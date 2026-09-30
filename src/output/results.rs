//! The search result as a table (SPEC §3.4, §11). All text in the table is Norwegian.
//!
//! The Trend column is left out when no row has enough history for it.

use comfy_table::{Attribute, Cell, CellAlignment, Color, Table};
use jiff::Timestamp;

use super::{format, table};
use crate::history::deals;
use crate::model::{DealBadge, PriceBasis, SearchResult, Verdict};
use crate::pricing::effective_unit_price;
use crate::search::ranking::{MAX_AGE_HOURS, SearchHits};
use crate::sources::{SourceState, SourceStatus};

/// Below this width the Pant column is dropped.
const NARROW: u16 = 80;
/// Below this width the Trend column is dropped.
const WITHOUT_TREND: u16 = 100;
/// Rows older than this show their age after the chain and are dimmed.
const OLD_HOURS: u32 = 24;

/// Builds the table. `width` is the terminal width (`None` when the output is not a
/// terminal), and `color` whether cells are styled.
pub fn table(hits: &SearchHits, color: bool, width: Option<u16>) -> Table {
    let narrow = width.is_some_and(|w| w < NARROW);
    let mut headers = vec!["Produkt", "Str.", "Kjede", "Pris", "Kr/L"];
    if !narrow {
        headers.push("Pant");
    }
    headers.push("Tilbud");
    headers.push("Vurdering");
    let trend =
        width.is_none_or(|w| w >= WITHOUT_TREND) && hits.rows.iter().any(|r| !r.trend.is_empty());
    if trend {
        headers.push("Trend");
    }

    let mut t = table::new(&headers);
    if let Some(width) = width {
        t.set_width(width);
    }
    if color {
        t.enforce_styling();
    }
    for column in [3, 4, 5] {
        if let Some(c) = t.column_mut(column)
            && (column < 5 || !narrow)
        {
            c.set_cell_alignment(CellAlignment::Right);
        }
    }

    for (i, row) in hits.rows.iter().enumerate() {
        let (deal, campaign) = deal_text(row);
        let verdict = verdict_text(row);
        let mut cells = vec![
            product_name(row),
            format::liters(row.product.volume),
            chain_name(row),
            format::kr(row.effective_unit_price),
            format::kr(row.liter_price),
        ];
        if !narrow {
            cells.push(format!("+{}", format::kr(row.deposit)));
        }
        cells.push(deal);
        cells.push(verdict.to_owned());
        let verdict_column = cells.len() - 1;
        if trend {
            cells.push(format::sparkline(&row.trend));
        }

        let dimmed = row.age_hours >= OLD_HOURS || !row.available;
        let deal_column = verdict_column - 1;
        let cells: Vec<Cell> = cells
            .into_iter()
            .enumerate()
            .map(|(j, text)| {
                let mut cell = Cell::new(text);
                if color {
                    if i == 0 {
                        cell = cell.add_attribute(Attribute::Bold);
                    }
                    if dimmed {
                        cell = cell.add_attribute(Attribute::Dim);
                    }
                    if j == deal_column && campaign {
                        cell = cell.fg(Color::Yellow);
                    }
                    if j == verdict_column
                        && let Some(color) = verdict_color(row.verdict.value)
                    {
                        cell = cell.fg(color);
                    }
                }
                cell
            })
            .collect();
        t.add_row(cells);
    }
    t
}

/// "Viser 20 av 47 treff · priser hentet for 2 t siden · 3 eldre enn 14 dager skjult …"
pub fn footer(hits: &SearchHits, sources: &[SourceStatus], now: Timestamp) -> String {
    let mut parts = vec![format!("Viser {} av {} treff", hits.rows.len(), hits.total)];
    if let Some(fetched) = sources
        .iter()
        .filter(|s| s.status == SourceState::Ok)
        .filter_map(|s| s.fetched)
        .max()
    {
        let hours = u32::try_from(now.duration_since(fetched).as_hours()).unwrap_or(0);
        parts.push(format!("priser hentet for {} siden", format::age(hours)));
    }
    let hidden = hits.hidden;
    if hidden.stale > 0 {
        parts.push(format!(
            "{} eldre enn {} dager skjult",
            hidden.stale,
            MAX_AGE_HOURS / 24
        ));
    }
    if hidden.sold_out > 0 {
        parts.push(format!("{} utsolgte skjult", hidden.sold_out));
    }
    if hidden.suspicious > 0 {
        parts.push(format!("{} mistenkelige skjult", hidden.suspicious));
    }
    if hidden.total() > 0 {
        parts.push("--alle viser alt".into());
    }
    let failed = sources
        .iter()
        .filter(|s| s.status == SourceState::Failed)
        .count();
    match failed {
        0 => {}
        1 => parts.push("1 kilde feilet (se over)".into()),
        n => parts.push(format!("{n} kilder feilet (se over)")),
    }
    parts.join(" · ")
}

fn product_name(row: &SearchResult) -> String {
    if row.product.verified {
        row.product.name.clone()
    } else {
        format!("{} ?", row.product.name)
    }
}

fn chain_name(row: &SearchResult) -> String {
    if row.age_hours >= OLD_HOURS {
        format!(
            "{} {}",
            row.chain.display_name(),
            format::age(row.age_hours)
        )
    } else {
        row.chain.display_name().to_owned()
    }
}

/// The text in the Vurdering column. A row that is not a deal shows `–` unless its
/// price is below the 90-day median, and `UKJENT` only matters for deals (SPEC §7.6).
fn verdict_text(row: &SearchResult) -> &'static str {
    let is_deal = row.deal_badge.is_some();
    let below_median = row.verdict.m90_ore.is_some_and(|m90| row.liter_price < m90);
    match row.verdict.value {
        Verdict::Unknown if !is_deal => "–",
        _ if !is_deal && !below_median => "–",
        verdict => verdict.label(),
    }
}

fn verdict_color(verdict: Verdict) -> Option<Color> {
    match verdict {
        Verdict::Great => Some(Color::Green),
        Verdict::Good => Some(Color::Cyan),
        Verdict::Fake => Some(Color::Red),
        Verdict::Fair | Verdict::Unknown => None,
    }
}

/// The text in the Tilbud column, and whether it is a campaign (shown in yellow).
fn deal_text(row: &SearchResult) -> (String, bool) {
    let mut parts = Vec::new();
    let campaign = row.deal_badge == Some(DealBadge::Campaign);
    match row.deal_badge {
        Some(DealBadge::Campaign) => {
            let mut text = String::from("KAMPANJE");
            if row.min_quantity > row.pack_size {
                text.push_str(&format!(" {}stk", row.min_quantity));
            }
            // Only with `tilbud --kommende`: the offer has not started yet.
            if let Some(start) = row.offer.as_ref().and_then(|o| o.valid_from)
                && start > deals::today_oslo()
            {
                text.push_str(&format!(" fra {}.{}.", start.day(), start.month()));
            }
            parts.push(text);
        }
        Some(DealBadge::PriceDrop) => parts.push("PRISFALL".into()),
        None => {}
    }
    match (row.price_basis, row.member_price) {
        (PriceBasis::Member, _) => parts.push("MEDLEM".into()),
        (_, Some(member_price)) => {
            let per_can = effective_unit_price(member_price, row.pack_size, None).unit_price;
            parts.push(format!("MEDLEM {}", format::kr(per_can)));
        }
        _ => {}
    }
    if row.pack_size > 1 {
        parts.push(format!("{}-pakning", row.pack_size));
    }
    if !row.available {
        parts.push("utsolgt".into());
    }
    (parts.join(" · "), campaign)
}

#[cfg(test)]
mod tests {
    use jiff::ToSpan;

    use super::*;
    use crate::model::{Ore, SourceId};
    use crate::test_support::{example_hits as example, now};

    #[test]
    fn table_without_colors() {
        insta::assert_snapshot!(table(&example(), false, Some(120)).to_string());
    }

    #[test]
    fn verdict_is_shown_for_deals_and_prices_below_the_median() {
        let mut row = example().rows[0].clone();
        row.deal_badge = None;
        row.verdict.value = Verdict::Great;
        row.verdict.m90_ore = Some(row.liter_price);
        assert_eq!(verdict_text(&row), "–");
        row.verdict.m90_ore = Some(Ore(row.liter_price.0 + 1));
        assert_eq!(verdict_text(&row), "SUPERT");

        row.verdict.value = Verdict::Unknown;
        assert_eq!(verdict_text(&row), "–");
        row.deal_badge = Some(DealBadge::Campaign);
        assert_eq!(verdict_text(&row), "UKJENT");
        row.verdict.value = Verdict::Fake;
        assert_eq!(verdict_text(&row), "LURERI");
    }

    #[test]
    fn upcoming_campaign_shows_its_start() {
        let mut row = example().rows[0].clone();
        if let Some(offer) = row.offer.as_mut() {
            offer.valid_from = Some(jiff::civil::date(2099, 10, 2));
        }
        assert_eq!(deal_text(&row).0, "KAMPANJE 3stk fra 2.10.");
    }

    #[test]
    fn trend_column_only_with_history_and_room() {
        let mut hits = example();
        assert!(!table(&hits, false, Some(120)).to_string().contains("Trend"));

        hits.rows[0].trend = vec![Some(Ore(4000)), Some(Ore(3000))];
        let wide = table(&hits, false, Some(120)).to_string();
        assert!(wide.contains("Trend"));
        assert!(wide.contains("█▁"));
        assert!(!table(&hits, false, Some(99)).to_string().contains("Trend"));
    }

    #[test]
    fn narrow_table_drops_deposit() {
        let text = table(&example(), false, Some(70)).to_string();
        assert!(!text.contains("Pant"));
        assert!(text.contains("Tilbud"));
    }

    #[test]
    fn footer_with_hidden_rows_and_failed_sources() {
        let sources = [
            SourceStatus {
                id: SourceId::Kassalapp,
                status: SourceState::Ok,
                fetched: Some(now() - 2.hours()),
                error: None,
                new_listings: None,
                matched: None,
            },
            SourceStatus {
                id: SourceId::Oda,
                status: SourceState::Failed,
                fetched: None,
                error: Some("HTTP 503".into()),
                new_listings: None,
                matched: None,
            },
        ];
        assert_eq!(
            footer(&example(), &sources, now()),
            "Viser 4 av 4 treff · priser hentet for 2 t siden · \
             1 eldre enn 14 dager skjult · --alle viser alt · 1 kilde feilet (se over)"
        );
    }
}
