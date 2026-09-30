//! One price per product, chain and pack size from several listings (SPEC §4.3).
//!
//! A chain can have several listings of the same thing: from different sources (Oda
//! directly and through Kassalapp), or several entries in one source. The rules:
//!
//! 1. The freshest price wins. Prices less than [`SAME_TIME`] apart count as a tie, and a
//!    direct source beats Kassalapp.
//! 2. An active offer from a direct source is layered on top of the winning price, and wins
//!    over a different offer from Kassalapp.
//! 3. Within one source, among the listings seen at the same time as its newest one, the
//!    most common shelf price wins, or else the median (the lower middle one, so it is a
//!    price that exists). When they disagree, the spread is kept as `prisspenn`.
//!
//! A single can and a 4-pack are different things and are never merged (SPEC §5.2).

use std::collections::HashMap;

use jiff::SignedDuration;
use jiff::civil::Date;

use crate::db::StoredPrice;
use crate::history::deals;
use crate::model::{Ore, PriceRange};

/// Observations less than this apart count as made at the same time. The daily fetch
/// makes a day the natural resolution.
pub const SAME_TIME: SignedDuration = SignedDuration::from_hours(24);
/// A direct source's offer is only layered on a fresher price if the direct source saw it
/// at most this long before: an offer without an end date seen long ago may be over.
pub const OFFER_MAX_AGE: SignedDuration = SignedDuration::from_hours(3 * 24);

/// The price that represents a group of listings.
#[derive(Debug, Clone, PartialEq)]
pub struct Merged {
    /// The winning listing's price, possibly with a direct source's offer on top.
    pub price: StoredPrice,
    /// The spread of shelf prices within the winning source, when they disagree.
    pub range: Option<PriceRange>,
}

pub fn merge(prices: &[StoredPrice], today: Date) -> Vec<Merged> {
    let mut groups: HashMap<_, Vec<&StoredPrice>> = HashMap::new();
    for price in prices {
        groups
            .entry((&price.product, price.chain, price.pack_size))
            .or_default()
            .push(price);
    }
    let mut merged: Vec<Merged> = groups
        .into_values()
        .map(|group| merge_group(&group, today))
        .collect();
    // A stable order, whatever the hash map did.
    merged.sort_by_key(|m| m.price.listing_id);
    merged
}

fn merge_group(group: &[&StoredPrice], today: Date) -> Merged {
    // Suspicious prices only count when there is nothing else.
    let trusted: Vec<&StoredPrice> = group.iter().copied().filter(|p| !p.suspicious).collect();
    let group = if trusted.is_empty() { group } else { &trusted };

    let mut by_source: HashMap<_, Vec<&StoredPrice>> = HashMap::new();
    for &price in group {
        by_source.entry(price.source).or_default().push(price);
    }
    let candidates: Vec<Merged> = by_source
        .into_values()
        .map(|listings| within_source(&listings))
        .collect();

    // Rule 1: the freshest, with a direct source winning a tie.
    let newest = candidates
        .iter()
        .map(|c| c.price.last_seen)
        .max()
        .expect("a group has at least one price");
    let mut winner = candidates
        .iter()
        .filter(|c| newest.duration_since(c.price.last_seen) < SAME_TIME)
        .max_by_key(|c| {
            (
                c.price.source.is_direct(),
                c.price.last_seen,
                c.price.listing_id,
            )
        })
        .cloned()
        .expect("the newest candidate is within its own window");

    // Rule 2: a direct source's active offer on top.
    let direct_offer = candidates
        .iter()
        .filter(|c| c.price.source.is_direct())
        .filter(|c| newest.duration_since(c.price.last_seen) <= OFFER_MAX_AGE)
        .filter_map(|c| c.price.offer.as_ref().map(|o| (c.price.source, o)))
        .find(|(_, offer)| deals::campaign_active(offer, today));
    if let Some((source, offer)) = direct_offer
        && winner.price.offer.as_ref() != Some(offer)
    {
        if winner.price.offer.is_some() {
            tracing::debug!(
                "{} hos {}: {} og {} melder ulike tilbud, bruker {}s",
                winner.price.product.0,
                winner.price.chain.slug(),
                winner.price.source.slug(),
                source.slug(),
                source.slug()
            );
        }
        winner.price.offer = Some(offer.clone());
    }
    winner
}

/// Rule 3: one price from one source's listings.
fn within_source(listings: &[&StoredPrice]) -> Merged {
    let newest = listings
        .iter()
        .map(|p| p.last_seen)
        .max()
        .expect("a source has at least one listing");
    let current: Vec<&StoredPrice> = listings
        .iter()
        .copied()
        .filter(|p| newest.duration_since(p.last_seen) < SAME_TIME)
        .collect();

    let shelf_prices: Vec<Ore> = current.iter().map(|p| p.shelf_price).collect();
    let consensus = consensus(&shelf_prices);
    let price = current
        .iter()
        .filter(|p| p.shelf_price == consensus)
        .max_by_key(|p| (p.last_seen, p.listing_id))
        .copied()
        .expect("the consensus is one of the prices");

    let low = shelf_prices.iter().copied().min();
    let high = shelf_prices.iter().copied().max();
    let range = match (low, high) {
        (Some(min_ore), Some(max_ore)) if min_ore != max_ore => {
            Some(PriceRange { min_ore, max_ore })
        }
        _ => None,
    };
    Merged {
        price: price.clone(),
        range,
    }
}

/// The most common price, or the median when no price is more common than the others
/// (the lower middle one for an even count).
fn consensus(prices: &[Ore]) -> Ore {
    let mut counts: HashMap<Ore, usize> = HashMap::new();
    for &price in prices {
        *counts.entry(price).or_default() += 1;
    }
    let top = counts.values().copied().max().unwrap_or(0);
    let most_common: Vec<Ore> = counts
        .iter()
        .filter(|&(_, &count)| count == top)
        .map(|(&price, _)| price)
        .collect();
    if let [only] = most_common.as_slice() {
        return *only;
    }
    let mut sorted = prices.to_vec();
    sorted.sort();
    sorted[(sorted.len() - 1) / 2]
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use jiff::{Timestamp, ToSpan};

    use super::*;
    use crate::model::{Chain, Offer, OfferInfo, ProductId, SourceId};

    fn now() -> Timestamp {
        "2026-09-30T12:00:00Z".parse().unwrap()
    }

    fn price(id: i64, source: SourceId, kr: i64, hours_ago: i64) -> StoredPrice {
        StoredPrice {
            listing_id: id,
            interval_id: id * 10,
            source,
            chain: Chain::Oda,
            product: ProductId("monster-ultra-white-500-boks".into()),
            pack_size: 1,
            verified: true,
            shelf_price: Ore(kr * 100),
            member_price: None,
            offer: None,
            available: Some(true),
            suspicious: false,
            last_seen: now() - hours_ago.hours(),
        }
    }

    fn one(prices: &[StoredPrice]) -> Merged {
        let merged = merge(prices, date(2026, 9, 30));
        assert_eq!(merged.len(), 1, "{merged:#?}");
        merged.into_iter().next().unwrap()
    }

    fn three_for_two() -> OfferInfo {
        OfferInfo {
            offer: Offer::NForM { n: 3, m: 2 },
            valid_from: None,
            valid_to: None,
            source_flagged: true,
        }
    }

    #[test]
    fn a_single_listing_is_itself() {
        let p = price(1, SourceId::Kassalapp, 30, 2);
        assert_eq!(
            one(std::slice::from_ref(&p)),
            Merged {
                price: p,
                range: None
            }
        );
    }

    #[test]
    fn the_freshest_source_wins() {
        // Kassalapp's Oda row is a year old; Oda itself was fetched today.
        let merged = one(&[
            price(1, SourceId::Kassalapp, 25, 24 * 365),
            price(2, SourceId::Oda, 30, 3),
        ]);
        assert_eq!(merged.price.listing_id, 2);
    }

    #[test]
    fn a_direct_source_wins_a_tie() {
        // Kassalapp saw it two hours after Oda: the same time.
        let merged = one(&[
            price(1, SourceId::Kassalapp, 25, 1),
            price(2, SourceId::Oda, 30, 3),
        ]);
        assert_eq!(merged.price.listing_id, 2);
        // A day and more apart: the fresher one.
        let merged = one(&[
            price(1, SourceId::Kassalapp, 25, 1),
            price(2, SourceId::Oda, 30, 26),
        ]);
        assert_eq!(merged.price.listing_id, 1);
    }

    #[test]
    fn a_direct_offer_is_layered_on_a_fresher_price() {
        let mut oda = price(2, SourceId::Oda, 30, 30);
        oda.offer = Some(three_for_two());
        let merged = one(&[price(1, SourceId::Kassalapp, 28, 1), oda]);
        assert_eq!(merged.price.listing_id, 1);
        assert_eq!(merged.price.shelf_price, Ore(2800));
        assert_eq!(merged.price.offer, Some(three_for_two()));
    }

    #[test]
    fn an_expired_direct_offer_is_not_layered() {
        let mut oda = price(2, SourceId::Oda, 30, 30);
        oda.offer = Some(OfferInfo {
            valid_to: Some(date(2026, 9, 29)),
            ..three_for_two()
        });
        let merged = one(&[price(1, SourceId::Kassalapp, 28, 1), oda]);
        assert_eq!(merged.price.offer, None);
    }

    #[test]
    fn an_old_direct_offer_is_not_layered() {
        // Oda last seen four days ago, with an offer that has no end date.
        let mut oda = price(2, SourceId::Oda, 30, 4 * 24 + 1);
        oda.offer = Some(three_for_two());
        let merged = one(&[price(1, SourceId::Kassalapp, 28, 1), oda]);
        assert_eq!(merged.price.offer, None);
    }

    #[test]
    fn the_most_common_price_within_a_source() {
        let merged = one(&[
            price(1, SourceId::Kassalapp, 25, 2),
            price(2, SourceId::Kassalapp, 29, 3),
            price(3, SourceId::Kassalapp, 29, 4),
        ]);
        assert_eq!(merged.price.shelf_price, Ore(2900));
        assert_eq!(merged.price.listing_id, 2);
        assert_eq!(
            merged.range,
            Some(PriceRange {
                min_ore: Ore(2500),
                max_ore: Ore(2900)
            })
        );
    }

    #[test]
    fn without_a_mode_the_lower_median() {
        let merged = one(&[
            price(1, SourceId::Kassalapp, 25, 2),
            price(2, SourceId::Kassalapp, 29, 3),
            price(3, SourceId::Kassalapp, 27, 4),
            price(4, SourceId::Kassalapp, 31, 5),
        ]);
        assert_eq!(merged.price.shelf_price, Ore(2700));
    }

    #[test]
    fn old_listings_do_not_count_within_a_source() {
        // As in the real data: an entry from June next to one from today.
        let merged = one(&[
            price(1, SourceId::Kassalapp, 24, 24 * 115),
            price(2, SourceId::Kassalapp, 15, 5),
        ]);
        assert_eq!(merged.price.listing_id, 2);
        assert_eq!(merged.range, None);
    }

    #[test]
    fn suspicious_prices_only_when_nothing_else() {
        let mut odd = price(1, SourceId::Kassalapp, 2, 1);
        odd.suspicious = true;
        let merged = one(&[odd.clone(), price(2, SourceId::Kassalapp, 30, 5)]);
        assert_eq!(merged.price.listing_id, 2);
        assert_eq!(one(&[odd]).price.listing_id, 1);
    }

    #[test]
    fn packs_and_chains_stay_apart() {
        let mut pack = price(2, SourceId::Kassalapp, 99, 1);
        pack.pack_size = 4;
        let mut kiwi = price(3, SourceId::Kassalapp, 30, 1);
        kiwi.chain = Chain::Kiwi;
        let merged = merge(
            &[price(1, SourceId::Kassalapp, 30, 1), pack, kiwi],
            date(2026, 9, 30),
        );
        assert_eq!(merged.len(), 3);
    }
}
