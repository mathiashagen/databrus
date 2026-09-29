//! Norwegian number formatting for human-readable output (SPEC §2). JSON never uses this.

use crate::model::{Ml, Ore};

/// A non-breaking space as the thousands separator.
const THOUSANDS_SEPARATOR: char = '\u{a0}';

/// `Ore(2490)` → `24,90`, `Ore(124900)` → `1 249,00`.
pub fn kr(amount: Ore) -> String {
    let sign = if amount.0 < 0 { "-" } else { "" };
    let abs = amount.0.unsigned_abs();
    format!("{sign}{},{:02}", group_thousands(abs / 100), abs % 100)
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
