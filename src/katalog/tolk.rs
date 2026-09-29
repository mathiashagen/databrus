//! Tolking av produktnavn, størrelser og pakninger (SPEC §6.3, §3.2).

use crate::modell::Ml;

/// Normaliserer tekst for sammenligning: små bokstaver, æøå foldet til ae/o/a, og alt
/// som ikke er bokstaver eller sifre blir ett mellomrom.
pub fn normaliser(tekst: &str) -> String {
    let mut ut = String::with_capacity(tekst.len());
    let mut mellomrom = false;
    for tegn in tekst.chars().flat_map(char::to_lowercase) {
        let erstatning = match tegn {
            'æ' => "ae",
            'ø' => "o",
            'å' => "a",
            t if t.is_alphanumeric() => {
                if mellomrom && !ut.is_empty() {
                    ut.push(' ');
                }
                mellomrom = false;
                ut.push(t);
                continue;
            }
            _ => {
                mellomrom = true;
                continue;
            }
        };
        if mellomrom && !ut.is_empty() {
            ut.push(' ');
        }
        mellomrom = false;
        ut.push_str(erstatning);
    }
    ut
}

/// Tolker `--storrelse`: `0,5`, `0.5`, `500ml`, `500`, `0,33l`, `50cl`.
/// Tall uten enhet opp til 5 er liter, større tall er ml.
pub fn tolk_storrelse(tekst: &str) -> Result<Ml, String> {
    let feil = || format!("ugyldig størrelse «{tekst}» – bruk f.eks. 0,5, 500ml eller 0,33l");
    let renset: String = tekst
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let delepunkt = renset
        .find(|c: char| !(c.is_ascii_digit() || c == ',' || c == '.'))
        .unwrap_or(renset.len());
    let (tall, enhet) = renset.split_at(delepunkt);
    let verdi: f64 = tall.replace(',', ".").parse().map_err(|_| feil())?;

    let ml = match enhet {
        "" if verdi <= 5.0 => verdi * 1000.0,
        "" | "ml" => verdi,
        "l" | "liter" => verdi * 1000.0,
        "dl" => verdi * 100.0,
        "cl" => verdi * 10.0,
        _ => return Err(feil()),
    };
    if !ml.is_finite() || !(1.0..=100_000.0).contains(&ml) {
        return Err(feil());
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ml::new(ml.round() as u32).ok_or_else(feil)
}

/// Finner volumet per beholder i et produktnavn, f.eks. «Monster Energy 0,5l boks»
/// eller «4x0,5 l». Tall uten volumenhet ignoreres.
pub fn tolk_volum(tekst: &str) -> Option<Ml> {
    let tegn: Vec<char> = tekst.to_lowercase().chars().collect();
    let mut i = 0;
    while i < tegn.len() {
        if !tallstart(&tegn, i) {
            i += 1;
            continue;
        }
        let (tall, etter_tall) = les_tall(&tegn, i);
        let (ord, _) = les_ord(&tegn, hopp_over(&tegn, etter_tall, &[' ']));
        // «0,5lx4» leses som ordet «lx»; x-en hører til pakningen.
        let enhet = ord.strip_suffix('x').unwrap_or(&ord);
        if matches!(enhet, "ml" | "cl" | "dl" | "l" | "liter")
            && let Ok(ml) = tolk_storrelse(&format!("{tall}{enhet}"))
        {
            return Some(ml);
        }
        i = etter_tall;
    }
    None
}

/// Finner antall beholdere i en flerpakning, f.eks. «4x0,5l», «0,5lx4», «24-pk» eller «6 pk».
pub fn tolk_pakke(tekst: &str) -> Option<u32> {
    let tegn: Vec<char> = tekst.to_lowercase().chars().collect();
    let mut i = 0;
    while i < tegn.len() {
        if !tallstart(&tegn, i) {
            i += 1;
            continue;
        }
        let (tall, etter_tall) = les_tall(&tegn, i);
        let (ord, _) = les_ord(&tegn, hopp_over(&tegn, etter_tall, &[' ', '-']));
        let er_pakke = matches!(
            ord.as_str(),
            "x" | "pk" | "pakk" | "pakning" | "pack" | "stk"
        ) || etter_volum_og_x(&tegn, i);
        if er_pakke
            && let Ok(antall) = tall.parse::<u32>()
            && (2..=48).contains(&antall)
        {
            return Some(antall);
        }
        i = etter_tall;
    }
    None
}

/// Om tallet som starter på `i` står etter «<volum>x», som i «0,5lx4» eller «0,5 l x 4».
fn etter_volum_og_x(tegn: &[char], i: usize) -> bool {
    let for_tallet: String = tegn[..i].iter().collect();
    let for_tallet = for_tallet.trim_end();
    let Some(for_x) = for_tallet.strip_suffix('x') else {
        return false;
    };
    let for_x = for_x.trim_end();
    ["ml", "cl", "dl", "l"].iter().any(|enhet| {
        for_x
            .strip_suffix(enhet)
            .is_some_and(|r| r.ends_with(|c: char| c.is_ascii_digit() || c == ' '))
    })
}

fn er_talltegn(c: char) -> bool {
    c.is_ascii_digit() || c == ',' || c == '.'
}

fn tallstart(tegn: &[char], i: usize) -> bool {
    tegn[i].is_ascii_digit() && (i == 0 || !er_talltegn(tegn[i - 1]))
}

fn les_tall(tegn: &[char], start: usize) -> (String, usize) {
    let mut slutt = start;
    while slutt < tegn.len() && er_talltegn(tegn[slutt]) {
        slutt += 1;
    }
    // Et avsluttende komma eller punktum («0,5l.») hører ikke til tallet.
    while slutt > start && !tegn[slutt - 1].is_ascii_digit() {
        slutt -= 1;
    }
    (tegn[start..slutt].iter().collect(), slutt)
}

fn les_ord(tegn: &[char], start: usize) -> (String, usize) {
    let mut slutt = start;
    while slutt < tegn.len() && tegn[slutt].is_alphabetic() {
        slutt += 1;
    }
    (tegn[start..slutt].iter().collect(), slutt)
}

fn hopp_over(tegn: &[char], mut i: usize, hopp: &[char]) -> usize {
    while i < tegn.len() && hopp.contains(&tegn[i]) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ml(tekst: &str) -> Option<u32> {
        tolk_storrelse(tekst).ok().map(Ml::get)
    }

    #[test]
    fn normalisering() {
        assert_eq!(normaliser("  Red-Bull  Sukkerfri!"), "red bull sukkerfri");
        assert_eq!(normaliser("Jordbær & Blåbær"), "jordbaer blabaer");
        assert_eq!(normaliser("Grønn"), "gronn");
    }

    #[test]
    fn storrelse_fra_kommandolinjen() {
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
    fn ugyldige_storrelser() {
        assert_eq!(ml("0,33,0,5"), None);
        assert_eq!(ml("stor"), None);
        assert_eq!(ml("0"), None);
        assert_eq!(ml("5kg"), None);
        assert_eq!(ml(""), None);
    }

    #[test]
    fn volum_fra_produktnavn() {
        let v = |t: &str| tolk_volum(t).map(Ml::get);
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
    fn pakke_fra_produktnavn() {
        assert_eq!(tolk_pakke("Monster 4x0,5l"), Some(4));
        assert_eq!(tolk_pakke("Battery 4 x 0,33 l"), Some(4));
        assert_eq!(tolk_pakke("Red Bull 24-pk"), Some(24));
        assert_eq!(tolk_pakke("Nocco 6 pk"), Some(6));
        assert_eq!(tolk_pakke("Monster Energy 0,5l"), None);
        assert_eq!(tolk_pakke("Burn 1x0,5l"), None);
        assert_eq!(tolk_pakke("Monster Mango Loco 0,5lx4 boks"), Some(4));
        assert_eq!(tolk_pakke("Monster 0,5 l x 6"), Some(6));
        assert_eq!(tolk_pakke("Red Bull Energidrikk 250ml 4pk boks"), Some(4));
        assert_eq!(tolk_pakke("Red Bull 4stk x 0,25l, 1l"), Some(4));
        assert_eq!(tolk_pakke("Monster ultra peachy keen 24x0,5l"), Some(24));
        assert_eq!(tolk_pakke("Nocco 330ml"), None);
    }
}
