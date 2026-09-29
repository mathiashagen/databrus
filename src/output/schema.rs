//! The published JSON schema for `databrus --json` (SPEC §9.1).
//!
//! The schema is generated from the Rust types, so it follows their Norwegian serde names
//! automatically. It is committed as `schema/v{SCHEMA_VERSION}.json`, and a test keeps the
//! file current:
//!
//! - an additive change (a new field, a new chain) only requires regenerating the file:
//!   `DATABRUS_UPDATE_SCHEMA=1 cargo test published_schema`
//! - a breaking change (a field removed or renamed, a type changed, a field that may now
//!   be missing, an enum value removed) fails until `SCHEMA_VERSION` is bumped, which
//!   starts a new file.

use serde_json::Value;

use super::json::{Envelope, SCHEMA_VERSION};
use crate::search::SearchContent;

/// Where the schema for `version` is published.
pub fn schema_url(version: u32) -> String {
    format!("https://raw.githubusercontent.com/mathiashagen/databrus/main/schema/v{version}.json")
}

/// The schema for the document `databrus --json` prints.
///
/// The field descriptions come from English doc comments, so they are removed; the
/// schema's own title and description are Norwegian, like the rest of the user-facing
/// output.
pub fn search_schema() -> Value {
    // The serialize contract: a field is required when it is always written, so an
    // `Option` without `skip_serializing_if` is required (and may be null), and a field
    // with `skip_serializing_if` is optional. The default contract describes input instead.
    let schema = schemars::generate::SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<Envelope<SearchContent<'static>>>();
    let mut value = serde_json::to_value(schema).unwrap_or(Value::Null);
    strip_descriptions(&mut value);
    if let Value::Object(root) = &mut value {
        root.insert("$id".into(), Value::String(schema_url(SCHEMA_VERSION)));
        root.insert(
            "title".into(),
            Value::String("databrus søkeresultat".into()),
        );
        root.insert(
            "description".into(),
            Value::String(format!(
                "Dokumentet `databrus --json` skriver ut (skjemaversjon {SCHEMA_VERSION}). \
                 Beløp er heltall øre, volum heltall ml og tidspunkt RFC 3339 i UTC. \
                 Nye felt og nye verdier kan komme uten at skjemaversjonen økes; \
                 fjernede eller endrede felt gir ny skjemaversjon."
            )),
        );
    }
    value
}

fn strip_descriptions(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("description");
            map.values_mut().for_each(strip_descriptions);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_descriptions),
        _ => {}
    }
}

/// Changes from `old` to `new` that can break a program reading the output. Each entry
/// is a JSON path with an explanation.
///
/// The rule is the reader's view: output valid under `new` must still make sense to a
/// program written against `old`. So new fields and new enum values are fine, while a
/// removed field, a field that may now be missing, a widened type or a removed enum
/// value is not.
pub fn breaking_changes(old: &Value, new: &Value) -> Vec<String> {
    let mut found = Vec::new();
    compare(old, old, new, new, "$", &mut found, 0);
    found
}

const MAX_DEPTH: usize = 32;

fn compare(
    old_root: &Value,
    old: &Value,
    new_root: &Value,
    new: &Value,
    path: &str,
    found: &mut Vec<String>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let old = resolve(old_root, old);
    let new = resolve(new_root, new);
    let next = |o: &Value, n: &Value, p: &str, f: &mut Vec<String>| {
        compare(old_root, o, new_root, n, p, f, depth + 1);
    };

    // Types: the new output may only use types the old schema allowed.
    let old_types = types(old);
    let new_types = types(new);
    if !old_types.is_empty() {
        for t in &new_types {
            let allowed = old_types.contains(t)
                || (t == "integer" && old_types.contains(&"number".to_string()));
            if !allowed {
                found.push(format!("{path}: typen «{t}» er ny (før: {old_types:?})"));
            }
        }
    }

    // Enum values: removing one changes the meaning for readers that match on it.
    if let (Some(old_values), Some(new_values)) = (enum_values(old), enum_values(new)) {
        for v in old_values.iter().filter(|v| !new_values.contains(v)) {
            found.push(format!("{path}: verdien {v} er fjernet"));
        }
    }

    // Fields: every old field must still exist, and a required one must stay required.
    if let Some(old_props) = old.get("properties").and_then(Value::as_object) {
        let new_props = new.get("properties").and_then(Value::as_object);
        let new_required = required(new);
        for (key, old_prop) in old_props {
            let field = format!("{path}.{key}");
            match new_props.and_then(|p| p.get(key)) {
                None => found.push(format!("{field}: feltet er fjernet")),
                Some(new_prop) => {
                    if required(old).contains(key) && !new_required.contains(key) {
                        found.push(format!("{field}: feltet kan nå mangle"));
                    }
                    next(old_prop, new_prop, &field, found);
                }
            }
        }
    }

    if let (Some(old_items), Some(new_items)) = (old.get("items"), new.get("items")) {
        next(old_items, new_items, &format!("{path}[]"), found);
    }

    // Alternatives (Option<T>, tagged enums): each old alternative needs a counterpart,
    // found by its "type" tag when it has one, otherwise by position.
    for keyword in ["anyOf", "oneOf", "allOf"] {
        let (Some(old_alts), Some(new_alts)) = (
            old.get(keyword).and_then(Value::as_array),
            new.get(keyword).and_then(Value::as_array),
        ) else {
            continue;
        };
        for (i, old_alt) in old_alts.iter().enumerate() {
            let old_alt_resolved = resolve(old_root, old_alt);
            let counterpart = match tag(old_alt_resolved) {
                Some(t) => new_alts
                    .iter()
                    .find(|n| tag(resolve(new_root, n)).as_ref() == Some(&t)),
                None => new_alts.get(i),
            };
            match counterpart {
                Some(new_alt) => next(old_alt, new_alt, &format!("{path}<{keyword} {i}>"), found),
                None => found.push(format!("{path}: alternativ {i} i {keyword} er fjernet")),
            }
        }
    }
}

/// Follows a local `$ref` like `#/$defs/SearchResult`.
fn resolve<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    let mut current = schema;
    for _ in 0..MAX_DEPTH {
        let Some(reference) = current.get("$ref").and_then(Value::as_str) else {
            return current;
        };
        let Some(pointer) = reference.strip_prefix('#') else {
            return current;
        };
        match root.pointer(pointer) {
            Some(target) => current = target,
            None => return current,
        }
    }
    current
}

fn types(schema: &Value) -> Vec<String> {
    match schema.get("type") {
        Some(Value::String(t)) => vec![t.clone()],
        Some(Value::Array(ts)) => ts
            .iter()
            .filter_map(|t| t.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn enum_values(schema: &Value) -> Option<Vec<Value>> {
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return Some(values.clone());
    }
    schema.get("const").map(|v| vec![v.clone()])
}

fn required(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| {
            r.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The discriminator of a tagged-enum alternative: its `type` property's value.
fn tag(schema: &Value) -> Option<Value> {
    let property = schema.get("properties")?.get("type")?;
    enum_values(property).and_then(|v| v.into_iter().next())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    fn published_path(version: u32) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("schema")
            .join(format!("v{version}.json"))
    }

    fn render(schema: &Value) -> String {
        let mut text = serde_json::to_string_pretty(schema).unwrap();
        text.push('\n');
        text
    }

    /// Keeps `schema/v{SCHEMA_VERSION}.json` in step with the code. See the module docs.
    #[test]
    fn published_schema_is_current() {
        let path = published_path(SCHEMA_VERSION);
        let generated = search_schema();
        let committed = std::fs::read_to_string(&path)
            .ok()
            .map(|text| text.replace("\r\n", "\n"));
        let update = std::env::var_os("DATABRUS_UPDATE_SCHEMA").is_some();

        let breaking = committed
            .as_deref()
            .map(|text| breaking_changes(&serde_json::from_str(text).unwrap(), &generated))
            .unwrap_or_default();
        assert!(
            breaking.is_empty(),
            "breaking changes to the JSON output:\n  {}\n\nBump SCHEMA_VERSION in src/output/json.rs \
             to {} and run `DATABRUS_UPDATE_SCHEMA=1 cargo test published_schema` to create \
             schema/v{}.json.",
            breaking.join("\n  "),
            SCHEMA_VERSION + 1,
            SCHEMA_VERSION + 1
        );

        let rendered = render(&generated);
        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &rendered).unwrap();
            return;
        }
        assert!(
            committed.is_some(),
            "{} is missing – run `DATABRUS_UPDATE_SCHEMA=1 cargo test published_schema`",
            path.display()
        );
        assert!(
            committed.as_deref() == Some(rendered.as_str()),
            "the JSON output changed in a compatible way, but {} is out of date – run \
             `DATABRUS_UPDATE_SCHEMA=1 cargo test published_schema` and commit the file",
            path.display()
        );
    }

    /// Validates `document` against `schema` and lists the errors.
    pub(crate) fn validation_errors(schema: &Value, document: &Value) -> Vec<String> {
        let validator = jsonschema::validator_for(schema).expect("the schema compiles");
        validator
            .iter_errors(document)
            .map(|e| format!("{}: {e}", e.instance_path()))
            .collect()
    }

    /// Real output – with an offer, a member price, a pack, an unverified match and a
    /// failed source – must validate against the generated schema.
    #[test]
    fn real_output_validates_against_the_schema() {
        use jiff::ToSpan;

        use crate::model::SourceId;
        use crate::search::{SearchContent, SearchFilter};
        use crate::sources::{SourceState, SourceStatus};
        use crate::test_support::{example_hits, now};

        let hits = example_hits();
        let filter = SearchFilter::from_args(
            &crate::cli::SearchArgs::default(),
            &crate::config::Config::default(),
        );
        let document = Envelope {
            header: super::super::json::Header::new().with_sources(vec![
                SourceStatus {
                    id: SourceId::Kassalapp,
                    status: SourceState::Ok,
                    fetched: Some(now() - 2.hours()),
                    error: None,
                    new_listings: Some(1),
                    matched: Some(1),
                },
                SourceStatus {
                    id: SourceId::Oda,
                    status: SourceState::Failed,
                    fetched: None,
                    error: Some("HTTP 503".into()),
                    new_listings: None,
                    matched: None,
                },
            ]),
            content: SearchContent {
                query: filter.query(&["monster".into()]),
                results: &hits.rows,
            },
        };
        let document = serde_json::to_value(&document).unwrap();
        assert!(
            document["resultater"][0]["tilbud"].is_object(),
            "the example has an offer"
        );
        let errors = validation_errors(&search_schema(), &document);
        assert!(errors.is_empty(), "{errors:#?}");

        // And the schema actually constrains something.
        let mut broken = document.clone();
        broken["resultater"][0]["kjede"] = json!("ukjent-kjede");
        broken["resultater"][0]["literpris_ore"] = json!("gratis");
        assert_eq!(validation_errors(&search_schema(), &broken).len(), 2);
    }

    #[test]
    fn schema_uses_the_norwegian_keys() {
        let schema = search_schema();
        let root = schema["properties"].as_object().unwrap();
        for key in [
            "skjemaversjon",
            "generert",
            "kilder",
            "sporring",
            "resultater",
        ] {
            assert!(root.contains_key(key), "missing {key}");
        }
        let result = resolve(&schema, &schema["properties"]["resultater"]["items"]);
        let props = result["properties"].as_object().unwrap();
        for key in ["literpris_ore", "pant_ore", "tilbudsmerke", "alder_timer"] {
            assert!(props.contains_key(key), "missing {key}");
        }
        assert!(!render(&schema).contains("\"description\": \"The "));
    }

    fn object(props: Value, required: &[&str]) -> Value {
        json!({"type": "object", "properties": props, "required": required})
    }

    #[test]
    fn a_new_field_is_not_breaking() {
        let old = object(json!({"a": {"type": "integer"}}), &["a"]);
        let new = object(
            json!({"a": {"type": "integer"}, "b": {"type": "string"}}),
            &["a", "b"],
        );
        assert!(breaking_changes(&old, &new).is_empty());
    }

    #[test]
    fn a_removed_or_renamed_field_is_breaking() {
        let old = object(json!({"a": {"type": "integer"}}), &["a"]);
        let new = object(json!({"b": {"type": "integer"}}), &["b"]);
        assert_eq!(breaking_changes(&old, &new), ["$.a: feltet er fjernet"]);
    }

    #[test]
    fn a_changed_or_widened_type_is_breaking() {
        let old = object(json!({"a": {"type": "integer"}}), &["a"]);
        let changed = object(json!({"a": {"type": "string"}}), &["a"]);
        let nullable = object(json!({"a": {"type": ["integer", "null"]}}), &["a"]);
        assert_eq!(breaking_changes(&old, &changed).len(), 1);
        assert_eq!(breaking_changes(&old, &nullable).len(), 1);
        // Narrowing is fine.
        assert!(breaking_changes(&nullable, &old).is_empty());
    }

    #[test]
    fn a_field_that_may_now_be_missing_is_breaking() {
        let old = object(json!({"a": {"type": "integer"}}), &["a"]);
        let new = object(json!({"a": {"type": "integer"}}), &[]);
        assert_eq!(breaking_changes(&old, &new), ["$.a: feltet kan nå mangle"]);
    }

    #[test]
    fn enum_values_may_be_added_but_not_removed() {
        let old = json!({"type": "string", "enum": ["kiwi", "rema"]});
        let added = json!({"type": "string", "enum": ["kiwi", "rema", "europris"]});
        let removed = json!({"type": "string", "enum": ["kiwi"]});
        assert!(breaking_changes(&old, &added).is_empty());
        assert_eq!(
            breaking_changes(&old, &removed),
            ["$: verdien \"rema\" er fjernet"]
        );
    }

    #[test]
    fn refs_are_followed_and_tagged_alternatives_matched_by_tag() {
        let old = json!({
            "$ref": "#/$defs/Offer",
            "$defs": {"Offer": {"oneOf": [
                object(json!({"type": {"const": "fastpris"}, "pris_ore": {"type": "integer"}}), &["type", "pris_ore"]),
                object(json!({"type": {"const": "prosent"}, "prosent": {"type": "integer"}}), &["type", "prosent"]),
            ]}}
        });
        // A new variant first, and the old ones reordered: still compatible.
        let new = json!({
            "$ref": "#/$defs/Tilbud",
            "$defs": {"Tilbud": {"oneOf": [
                object(json!({"type": {"const": "n_for_m"}, "n": {"type": "integer"}}), &["type", "n"]),
                object(json!({"type": {"const": "prosent"}, "prosent": {"type": "integer"}}), &["type", "prosent"]),
                object(json!({"type": {"const": "fastpris"}, "pris_ore": {"type": "integer"}}), &["type", "pris_ore"]),
            ]}}
        });
        assert!(
            breaking_changes(&old, &new).is_empty(),
            "{:?}",
            breaking_changes(&old, &new)
        );

        // Dropping a variant is breaking.
        let dropped = json!({"oneOf": [
            object(json!({"type": {"const": "prosent"}, "prosent": {"type": "integer"}}), &["type", "prosent"]),
        ]});
        assert_eq!(breaking_changes(&old, &dropped).len(), 1);
    }
}
