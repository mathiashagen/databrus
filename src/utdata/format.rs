//! Norsk tallformatering for lesbare utdata (SPEC §2). JSON bruker aldri dette.

use crate::modell::{Ml, Ore};

/// Hardt mellomrom som tusenskille.
const TUSENSKILLE: char = '\u{a0}';

/// `Ore(2490)` → `24,90`, `Ore(124900)` → `1 249,00`.
pub fn kr(belop: Ore) -> String {
    let fortegn = if belop.0 < 0 { "-" } else { "" };
    let abs = belop.0.unsigned_abs();
    format!("{fortegn}{},{:02}", tusenskill(abs / 100), abs % 100)
}

/// `500` → `0,5 l`, `330` → `0,33 l`, `1000` → `1 l`.
pub fn liter(volum: Ml) -> String {
    let ml = volum.get();
    let (hele, rest) = (ml / 1000, ml % 1000);
    if rest == 0 {
        format!("{hele} l")
    } else {
        let desimaler = format!("{rest:03}");
        format!("{hele},{} l", desimaler.trim_end_matches('0'))
    }
}

/// Alder som i tabellen: `2 t` under et døgn, ellers `3d`.
pub fn alder(timer: u32) -> String {
    if timer < 24 {
        format!("{timer} t")
    } else {
        format!("{}d", timer / 24)
    }
}

/// Tolker et kronebeløp fra kommandolinjen: `25`, `24,90`, `24.9`, `kr 25`.
pub fn tolk_kr(tekst: &str) -> Result<Ore, String> {
    let feil = || format!("ugyldig beløp «{tekst}» – bruk f.eks. 25 eller 24,90");
    let renset = tekst
        .trim()
        .trim_start_matches("kr")
        .trim_end_matches("kr")
        .trim()
        .replace(',', ".");
    let (hele, desimaler) = renset.split_once('.').unwrap_or((renset.as_str(), ""));
    let bare_sifre = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if hele.is_empty() || !bare_sifre(hele) || desimaler.len() > 2 || !bare_sifre(desimaler) {
        return Err(feil());
    }
    let kroner: i64 = hele.parse().map_err(|_| feil())?;
    let ore: i64 = match desimaler.len() {
        0 => 0,
        1 => desimaler.parse::<i64>().map_err(|_| feil())? * 10,
        _ => desimaler.parse().map_err(|_| feil())?,
    };
    kroner
        .checked_mul(100)
        .and_then(|k| k.checked_add(ore))
        .map(Ore)
        .ok_or_else(feil)
}

fn tusenskill(mut tall: u64) -> String {
    let mut grupper = Vec::new();
    while tall >= 1000 {
        grupper.push(format!("{:03}", tall % 1000));
        tall /= 1000;
    }
    grupper.push(tall.to_string());
    grupper.reverse();
    grupper.join(&TUSENSKILLE.to_string())
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
    fn liter_uten_overflodige_nuller() {
        let l = |ml| liter(Ml::new(ml).unwrap());
        assert_eq!(l(500), "0,5 l");
        assert_eq!(l(330), "0,33 l");
        assert_eq!(l(355), "0,355 l");
        assert_eq!(l(1000), "1 l");
        assert_eq!(l(1500), "1,5 l");
    }

    #[test]
    fn alder_i_timer_og_dager() {
        assert_eq!(alder(2), "2 t");
        assert_eq!(alder(72), "3d");
    }

    #[test]
    fn kronebelop_fra_kommandolinjen() {
        assert_eq!(tolk_kr("25"), Ok(Ore(2500)));
        assert_eq!(tolk_kr("24,90"), Ok(Ore(2490)));
        assert_eq!(tolk_kr("24.9"), Ok(Ore(2490)));
        assert_eq!(tolk_kr("kr 18"), Ok(Ore(1800)));
        assert!(tolk_kr("24,905").is_err());
        assert!(tolk_kr("-5").is_err());
        assert!(tolk_kr("femti").is_err());
        assert!(tolk_kr("").is_err());
    }
}
