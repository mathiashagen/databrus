//! Hva som regnes som et tilbud (SPEC §7.8).
//!
//! KAMPANJE (kilden merker et aktivt tilbud) er på plass. PRISFALL (≥ 10 % under
//! 90-dagersmedianen) trenger historikkstatistikken og kommer i M2.

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::modell::{Tilbudsinfo, Tilbudsmerke};

/// Dagens dato i Norge. Dagsgrenser følger alltid Europe/Oslo.
pub fn idag_oslo() -> Date {
    let na = Timestamp::now();
    na.in_tz("Europe/Oslo")
        .unwrap_or_else(|_| na.to_zoned(TimeZone::UTC))
        .date()
}

/// Om et kildemerket tilbud gjelder `dato` (begge ender inklusive).
pub fn kampanje_aktiv(tilbud: &Tilbudsinfo, dato: Date) -> bool {
    tilbud.kilde_merket
        && tilbud.gyldig_fra.is_none_or(|fra| fra <= dato)
        && tilbud.gyldig_til.is_none_or(|til| dato <= til)
}

/// Om et kildemerket tilbud starter etter `dato` (for `tilbud --kommende`).
pub fn kampanje_kommende(tilbud: &Tilbudsinfo, dato: Date) -> bool {
    tilbud.kilde_merket && tilbud.gyldig_fra.is_some_and(|fra| fra > dato)
}

pub fn tilbudsmerke(tilbud: Option<&Tilbudsinfo>, dato: Date) -> Option<Tilbudsmerke> {
    tilbud
        .filter(|t| kampanje_aktiv(t, dato))
        .map(|_| Tilbudsmerke::Kampanje)
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;
    use crate::modell::{Ore, Tilbud};

    fn tilbud(fra: Option<Date>, til: Option<Date>, kilde_merket: bool) -> Tilbudsinfo {
        Tilbudsinfo {
            tilbud: Tilbud::Fastpris {
                pris_ore: Ore(1590),
            },
            gyldig_fra: fra,
            gyldig_til: til,
            kilde_merket,
        }
    }

    #[test]
    fn gyldighet_er_inklusiv() {
        let t = tilbud(Some(date(2026, 9, 28)), Some(date(2026, 10, 4)), true);
        assert!(!kampanje_aktiv(&t, date(2026, 9, 27)));
        assert!(kampanje_aktiv(&t, date(2026, 9, 28)));
        assert!(kampanje_aktiv(&t, date(2026, 10, 4)));
        assert!(!kampanje_aktiv(&t, date(2026, 10, 5)));
        assert!(kampanje_kommende(&t, date(2026, 9, 27)));
    }

    #[test]
    fn apne_ender_gjelder_alltid() {
        assert!(kampanje_aktiv(&tilbud(None, None, true), date(2026, 1, 1)));
    }

    #[test]
    fn umerket_er_ikke_kampanje() {
        let t = tilbud(None, None, false);
        assert_eq!(tilbudsmerke(Some(&t), date(2026, 1, 1)), None);
    }
}
