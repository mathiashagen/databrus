//! The search result as a table (SPEC §3.4, §11). All text in the table is Norwegian.
//!
//! Vurdering and Trend come with the history in M2; until then they are not shown.

use comfy_table::{Attribute, Cell, CellAlignment, Color, Table};
use jiff::Timestamp;

use super::{format, table};
use crate::model::{DealBadge, PriceBasis, SearchResult};
use crate::pricing::effective_unit_price;
use crate::search::ranking::{MAX_AGE_HOURS, SearchHits};
use crate::sources::{SourceState, SourceStatus};

/// Below this width the Pant column is dropped.
const NARROW: u16 = 80;
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

        let dimmed = row.age_hours >= OLD_HOURS || !row.available;
        let last = cells.len() - 1;
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
                    if j == last && campaign {
                        cell = cell.fg(Color::Yellow);
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

/// The text in the Tilbud column, and whether it is a campaign (shown in yellow).
fn deal_text(row: &SearchResult) -> (String, bool) {
    let mut parts = Vec::new();
    let campaign = row.deal_badge == Some(DealBadge::Campaign);
    match row.deal_badge {
        Some(DealBadge::Campaign) if row.min_quantity > row.pack_size => {
            parts.push(format!("KAMPANJE {}stk", row.min_quantity));
        }
        Some(DealBadge::Campaign) => parts.push("KAMPANJE".into()),
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
    use jiff::civil::date;

    use super::*;
    use crate::catalog::Catalog;
    use crate::cli::SearchArgs;
    use crate::config::Config;
    use crate::db::StoredPrice;
    use crate::model::{
        Chain, MemberPrice, MembershipProgram, Offer, OfferInfo, Ore, ProductId, SourceId,
    };
    use crate::search::SearchFilter;
    use crate::search::ranking::rank;

    const CATALOG: &str = r#"
        [[produkt]]
        id = "monster-ultra-white-500-boks"
        navn = "Monster Ultra White"
        merke = "monster"
        smak = "ultra-white"
        sukkerfri = true
        volum_ml = 500
        beholder = "boks"

        [[produkt]]
        id = "red-bull-energy-drink-250-boks"
        navn = "Red Bull Energy Drink"
        merke = "red-bull"
        smak = "original"
        sukkerfri = false
        volum_ml = 250
        beholder = "boks"
    "#;

    fn now() -> Timestamp {
        "2026-09-29T12:00:00Z".parse().unwrap()
    }

    fn price(product: &str, chain: Chain, shelf_price: i64, hours_ago: i64) -> StoredPrice {
        StoredPrice {
            listing_id: 0,
            source: SourceId::Kassalapp,
            chain,
            product: ProductId(product.into()),
            pack_size: 1,
            verified: true,
            shelf_price: Ore(shelf_price),
            member_price: None,
            offer: None,
            available: None,
            suspicious: false,
            last_seen: now() - hours_ago.hours(),
        }
    }

    fn example() -> SearchHits {
        let mut campaign = price("monster-ultra-white-500-boks", Chain::Kiwi, 2490, 2);
        campaign.offer = Some(OfferInfo {
            offer: Offer::NForSum {
                n: 3,
                sum: Ore(4770),
            },
            valid_from: Some(date(2026, 9, 28)),
            valid_to: Some(date(2026, 10, 4)),
            source_flagged: true,
        });
        let mut member = price("monster-ultra-white-500-boks", Chain::CoopExtra, 2190, 3);
        member.member_price = Some(MemberPrice {
            price: Ore(1890),
            program: MembershipProgram::Coop,
        });
        let mut pack = price("red-bull-energy-drink-250-boks", Chain::Joker, 9960, 72);
        pack.pack_size = 4;
        let mut unverified = price("monster-ultra-white-500-boks", Chain::Oda, 2240, 5);
        unverified.verified = false;
        let stale = price(
            "red-bull-energy-drink-250-boks",
            Chain::Rema,
            1990,
            24 * 400,
        );

        let catalog = Catalog::from_toml(CATALOG).unwrap();
        let filter = SearchFilter::from_args(&SearchArgs::default(), &Config::default());
        rank(
            &[campaign, member, pack, unverified, stale],
            &catalog,
            &filter,
            &Config::default(),
            now(),
            date(2026, 9, 29),
        )
    }

    #[test]
    fn table_without_colors() {
        insta::assert_snapshot!(table(&example(), false, Some(120)).to_string());
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
