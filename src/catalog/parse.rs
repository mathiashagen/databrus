//! Parsing product names, sizes and pack sizes (SPEC §6.3, §3.2).
//!
//! The inputs are Norwegian store names and command-line values, so the recognized unit
//! and pack words are Norwegian ("stk", "pk", "pakning").

use crate::model::Ml;

/// Normalizes text for comparison: lowercase, æøå folded to ae/o/a, and anything that
/// is not a letter or digit becomes a single space.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars().flat_map(char::to_lowercase) {
        let replacement = match c {
            'æ' => "ae",
            'ø' => "o",
            'å' => "a",
            c if c.is_alphanumeric() => {
                if space && !out.is_empty() {
                    out.push(' ');
                }
                space = false;
                out.push(c);
                continue;
            }
            _ => {
                space = true;
                continue;
            }
        };
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push_str(replacement);
    }
    out
}

/// Parses `--storrelse`: `0,5`, `0.5`, `500ml`, `500`, `0,33l`, `50cl`.
/// Numbers without a unit up to 5 are liters, larger numbers are ml.
///
/// The error message is Norwegian, since clap shows it to the user.
pub fn parse_size(text: &str) -> Result<Ml, String> {
    let error = || format!("ugyldig størrelse «{text}» – bruk f.eks. 0,5, 500ml eller 0,33l");
    let cleaned: String = text
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let split = cleaned
        .find(|c: char| !(c.is_ascii_digit() || c == ',' || c == '.'))
        .unwrap_or(cleaned.len());
    let (number, unit) = cleaned.split_at(split);
    let value: f64 = number.replace(',', ".").parse().map_err(|_| error())?;

    let ml = match unit {
        "" if value <= 5.0 => value * 1000.0,
        "" | "ml" => value,
        "l" | "liter" => value * 1000.0,
        "dl" => value * 100.0,
        "cl" => value * 10.0,
        _ => return Err(error()),
    };
    if !ml.is_finite() || !(1.0..=100_000.0).contains(&ml) {
        return Err(error());
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ml::new(ml.round() as u32).ok_or_else(error)
}

/// Finds the volume per container in a product name, e.g. "Monster Energy 0,5l boks" or
/// "4x0,5 l". Numbers without a volume unit are ignored.
pub fn parse_volume(text: &str) -> Option<Ml> {
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !number_starts(&chars, i) {
            i += 1;
            continue;
        }
        let (number, after_number) = read_number(&chars, i);
        let (word, _) = read_word(&chars, skip(&chars, after_number, &[' ']));
        // "0,5lx4" reads as the word "lx"; the x belongs to the pack size.
        let unit = word.strip_suffix('x').unwrap_or(&word);
        if matches!(unit, "ml" | "cl" | "dl" | "l" | "liter")
            && let Ok(ml) = parse_size(&format!("{number}{unit}"))
        {
            return Some(ml);
        }
        i = after_number;
    }
    None
}

/// Finds the number of containers in a multipack, e.g. "4x0,5l", "0,5lx4", "24-pk" or
/// "6 pk".
pub fn parse_pack_size(text: &str) -> Option<u32> {
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !number_starts(&chars, i) {
            i += 1;
            continue;
        }
        let (number, after_number) = read_number(&chars, i);
        let (word, _) = read_word(&chars, skip(&chars, after_number, &[' ', '-']));
        let is_pack = matches!(
            word.as_str(),
            "x" | "pk" | "pakk" | "pakning" | "pack" | "stk"
        ) || follows_volume_and_x(&chars, i);
        if is_pack
            && let Ok(count) = number.parse::<u32>()
            && (2..=48).contains(&count)
        {
            return Some(count);
        }
        i = after_number;
    }
    None
}

/// Whether the number starting at `i` follows "<volume>x", as in "0,5lx4" or "0,5 l x 4".
fn follows_volume_and_x(chars: &[char], i: usize) -> bool {
    let before: String = chars[..i].iter().collect();
    let before = before.trim_end();
    let Some(before_x) = before.strip_suffix('x') else {
        return false;
    };
    let before_x = before_x.trim_end();
    ["ml", "cl", "dl", "l"].iter().any(|unit| {
        before_x
            .strip_suffix(unit)
            .is_some_and(|r| r.ends_with(|c: char| c.is_ascii_digit() || c == ' '))
    })
}

fn is_number_char(c: char) -> bool {
    c.is_ascii_digit() || c == ',' || c == '.'
}

fn number_starts(chars: &[char], i: usize) -> bool {
    chars[i].is_ascii_digit() && (i == 0 || !is_number_char(chars[i - 1]))
}

fn read_number(chars: &[char], start: usize) -> (String, usize) {
    let mut end = start;
    while end < chars.len() && is_number_char(chars[end]) {
        end += 1;
    }
    // A trailing comma or period ("0,5l.") is not part of the number.
    while end > start && !chars[end - 1].is_ascii_digit() {
        end -= 1;
    }
    (chars[start..end].iter().collect(), end)
}

fn read_word(chars: &[char], start: usize) -> (String, usize) {
    let mut end = start;
    while end < chars.len() && chars[end].is_alphabetic() {
        end += 1;
    }
    (chars[start..end].iter().collect(), end)
}

fn skip(chars: &[char], mut i: usize, skipped: &[char]) -> usize {
    while i < chars.len() && skipped.contains(&chars[i]) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ml(text: &str) -> Option<u32> {
        parse_size(text).ok().map(Ml::get)
    }

    #[test]
    fn normalization() {
        assert_eq!(normalize("  Red-Bull  Sukkerfri!"), "red bull sukkerfri");
        assert_eq!(normalize("Jordbær & Blåbær"), "jordbaer blabaer");
        assert_eq!(normalize("Grønn"), "gronn");
    }

    #[test]
    fn size_from_the_command_line() {
        assert_eq!(ml("0,5"), Some(500));
        assert_eq!(ml("0.5"), Some(500));
        assert_eq!(ml("500ml"), Some(500));
        assert_eq!(ml("500"), Some(500));
        assert_eq!(ml("0,33l"), Some(330));
        assert_eq!(ml("0,33 L"), Some(330));
        assert_eq!(ml("50cl"), Some(500));
        assert_eq!(ml("1"), Some(1000));
        assert_eq!(ml("355"), Some(355));
    }

    #[test]
    fn invalid_sizes() {
        assert_eq!(ml("0,33,0,5"), None);
        assert_eq!(ml("stor"), None);
        assert_eq!(ml("0"), None);
        assert_eq!(ml("5kg"), None);
        assert_eq!(ml(""), None);
    }

    #[test]
    fn volume_from_product_name() {
        let v = |t: &str| parse_volume(t).map(Ml::get);
        assert_eq!(v("Monster Energy 0,5l boks"), Some(500));
        assert_eq!(v("Red Bull 250 ml"), Some(250));
        assert_eq!(v("Nocco 0.33 L"), Some(330));
        assert_eq!(v("Burn 50cl."), Some(500));
        assert_eq!(v("Monster 4x0,5l"), Some(500));
        assert_eq!(v("Battery 4 x 0,33 l"), Some(330));
        assert_eq!(v("Energidrikk 2 for 50"), None);
        assert_eq!(v("Monster Mango Loco 0,5lx4 boks"), Some(500));
        assert_eq!(v("Red Bull Energidrikk 250ml 4pk boks"), Some(250));
        assert_eq!(v("Burn Original 4stk x 0,5l, 2l"), Some(500));
    }

    #[test]
    fn pack_size_from_product_name() {
        assert_eq!(parse_pack_size("Monster 4x0,5l"), Some(4));
        assert_eq!(parse_pack_size("Battery 4 x 0,33 l"), Some(4));
        assert_eq!(parse_pack_size("Red Bull 24-pk"), Some(24));
        assert_eq!(parse_pack_size("Nocco 6 pk"), Some(6));
        assert_eq!(parse_pack_size("Monster Energy 0,5l"), None);
        assert_eq!(parse_pack_size("Burn 1x0,5l"), None);
        assert_eq!(parse_pack_size("Monster Mango Loco 0,5lx4 boks"), Some(4));
        assert_eq!(parse_pack_size("Monster 0,5 l x 6"), Some(6));
        assert_eq!(
            parse_pack_size("Red Bull Energidrikk 250ml 4pk boks"),
            Some(4)
        );
        assert_eq!(parse_pack_size("Red Bull 4stk x 0,25l, 1l"), Some(4));
        assert_eq!(
            parse_pack_size("Monster ultra peachy keen 24x0,5l"),
            Some(24)
        );
        assert_eq!(parse_pack_size("Nocco 330ml"), None);
    }
}
