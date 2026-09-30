//! Norwegian number formatting for human-readable output (SPEC §2). JSON never uses this.

use crate::model::{Ml, Offer, Ore};

/// A non-breaking space as the thousands separator.
const THOUSANDS_SEPARATOR: char = '\u{a0}';

/// `Ore(2490)` → `24,90`, `Ore(124900)` → `1 249,00`.
pub fn kr(amount: Ore) -> String {
    let sign = if amount.0 < 0 { "-" } else { "" };
    let abs = amount.0.unsigned_abs();
    format!("{sign}{},{:02}", group_thousands(abs / 100), abs % 100)
}

/// An offer in words: `nå 15,90`, `3 for 2`, `2 for 50,00`, `30 % rabatt`,
/// `3. stk gratis`, `2. stk til halv pris`.
pub fn offer(offer: &Offer) -> String {
    match *offer {
        Offer::FixedPrice { price } => format!("nå {}", kr(price)),
        Offer::NForM { n, m } => format!("{n} for {m}"),
        Offer::NForSum { n, sum } => format!("{n} for {}", kr(sum)),
        Offer::Percent { percent } => format!("{percent} % rabatt"),
        Offer::NthItem {
            n,
            discount_percent: 100,
        } => format!("{n}. stk gratis"),
        Offer::NthItem {
            n,
            discount_percent: 50,
        } => format!("{n}. stk til halv pris"),
        Offer::NthItem {
            n,
            discount_percent,
        } => format!("{n}. stk {discount_percent} % rabatt"),
    }
}

const SPARK_LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// A sparkline, one character per value and a space where nothing is known. The scale
/// spans at least 5 % of the highest price, centered, so a few øre up or down doesn't look
/// like a big swing and a flat price sits in the middle (`▅▅▅▅▅▅`).
pub fn sparkline(values: &[Option<Ore>]) -> String {
    let known = values.iter().flatten().map(|o| o.0);
    let (Some(low), Some(high)) = (known.clone().min(), known.max()) else {
        return " ".repeat(values.len());
    };
    let span = (high - low).max(high.abs() / 20).max(1);
    let bottom = i128::from(low) - i128::from(span - (high - low)) / 2;
    values
        .iter()
        .map(|value| match value {
            None => ' ',
            Some(price) => {
                let top = SPARK_LEVELS.len() as i128 - 1;
                let level = crate::pricing::div_round(
                    (i128::from(price.0) - bottom) * top,
                    i128::from(span),
                );
                SPARK_LEVELS[usize::try_from(level.clamp(0, 7)).unwrap_or(0)]
            }
        })
        .collect()
}

/// `500` → `0,5 l`, `330` → `0,33 l`, `1000` → `1 l`.
pub fn liters(volume: Ml) -> String {
    let ml = volume.get();
    let (whole, rest) = (ml / 1000, ml % 1000);
    if rest == 0 {
        format!("{whole} l")
    } else {
        let decimals = format!("{rest:03}");
        format!("{whole},{} l", decimals.trim_end_matches('0'))
    }
}

/// Age as shown in the table: `2 t` below a day, otherwise `3d`.
pub fn age(hours: u32) -> String {
    if hours < 24 {
        format!("{hours} t")
    } else {
        format!("{}d", hours / 24)
    }
}

/// Parses a kroner amount from the command line: `25`, `24,90`, `24.9`, `kr 25`.
/// The error message is Norwegian, since clap shows it to the user.
pub fn parse_kr(text: &str) -> Result<Ore, String> {
    let error = || format!("ugyldig beløp «{text}» – bruk f.eks. 25 eller 24,90");
    let cleaned = text
        .trim()
        .trim_start_matches("kr")
        .trim_end_matches("kr")
        .trim()
        .replace(',', ".");
    let (whole, decimals) = cleaned.split_once('.').unwrap_or((cleaned.as_str(), ""));
    let only_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || !only_digits(whole) || decimals.len() > 2 || !only_digits(decimals) {
        return Err(error());
    }
    let kroner: i64 = whole.parse().map_err(|_| error())?;
    let ore: i64 = match decimals.len() {
        0 => 0,
        1 => decimals.parse::<i64>().map_err(|_| error())? * 10,
        _ => decimals.parse().map_err(|_| error())?,
    };
    kroner
        .checked_mul(100)
        .and_then(|k| k.checked_add(ore))
        .map(Ore)
        .ok_or_else(error)
}

fn group_thousands(mut number: u64) -> String {
    let mut groups = Vec::new();
    while number >= 1000 {
        groups.push(format!("{:03}", number % 1000));
        number /= 1000;
    }
    groups.push(number.to_string());
    groups.reverse();
    groups.join(&THOUSANDS_SEPARATOR.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_in_words() {
        assert_eq!(offer(&Offer::FixedPrice { price: Ore(1590) }), "nå 15,90");
        assert_eq!(offer(&Offer::NForM { n: 3, m: 2 }), "3 for 2");
        assert_eq!(
            offer(&Offer::NForSum {
                n: 2,
                sum: Ore(5000)
            }),
            "2 for 50,00"
        );
        assert_eq!(offer(&Offer::Percent { percent: 30 }), "30 % rabatt");
        let nth = |n, discount_percent| {
            offer(&Offer::NthItem {
                n,
                discount_percent,
            })
        };
        assert_eq!(nth(3, 100), "3. stk gratis");
        assert_eq!(nth(2, 50), "2. stk til halv pris");
        assert_eq!(nth(2, 30), "2. stk 30 % rabatt");
    }

    fn kr_values(values: &[Option<i64>]) -> Vec<Option<Ore>> {
        values.iter().map(|v| v.map(|kr| Ore(kr * 100))).collect()
    }

    #[test]
    fn sparkline_spans_low_to_high() {
        let values = kr_values(&[Some(50), Some(50), Some(40), Some(30), None, Some(30)]);
        assert_eq!(sparkline(&values), "██▅▁ ▁");
    }

    #[test]
    fn flat_sparkline_sits_in_the_middle() {
        assert_eq!(sparkline(&kr_values(&[Some(40); 6])), "▅▅▅▅▅▅");
        // 10 øre on 40 kr is at most one step, not a swing from ▁ to █.
        let values = [Some(Ore(4000)), Some(Ore(4010)), Some(Ore(4000))];
        assert_eq!(sparkline(&values), "▄▅▄");
    }

    #[test]
    fn empty_sparkline_is_blank() {
        assert_eq!(sparkline(&[None, None]), "  ");
        assert_eq!(sparkline(&[]), "");
    }

    #[test]
    fn kroner() {
        assert_eq!(kr(Ore(2490)), "24,90");
        assert_eq!(kr(Ore(5)), "0,05");
        assert_eq!(kr(Ore(124_900)), "1\u{a0}249,00");
        assert_eq!(kr(Ore(123_456_789)), "1\u{a0}234\u{a0}567,89");
        assert_eq!(kr(Ore(-200)), "-2,00");
    }

    #[test]
    fn liters_without_trailing_zeros() {
        let l = |ml| liters(Ml::new(ml).unwrap());
        assert_eq!(l(500), "0,5 l");
        assert_eq!(l(330), "0,33 l");
        assert_eq!(l(355), "0,355 l");
        assert_eq!(l(1000), "1 l");
        assert_eq!(l(1500), "1,5 l");
    }

    #[test]
    fn age_in_hours_and_days() {
        assert_eq!(age(2), "2 t");
        assert_eq!(age(72), "3d");
    }

    #[test]
    fn kroner_from_the_command_line() {
        assert_eq!(parse_kr("25"), Ok(Ore(2500)));
        assert_eq!(parse_kr("24,90"), Ok(Ore(2490)));
        assert_eq!(parse_kr("24.9"), Ok(Ore(2490)));
        assert_eq!(parse_kr("kr 18"), Ok(Ore(1800)));
        assert!(parse_kr("24,905").is_err());
        assert!(parse_kr("-5").is_err());
        assert!(parse_kr("femti").is_err());
        assert!(parse_kr("").is_err());
    }
}
