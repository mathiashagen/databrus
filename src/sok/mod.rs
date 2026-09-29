//! Filtrering og rangering (SPEC §3.2, §6.4).
//!
//! Filtrene på katalognivå er på plass. Rangering av prisrader kommer i M1.

use clap::ValueEnum;

use crate::cli::{SokArgs, utvid_kjeder};
use crate::katalog::tolk::normaliser;
use crate::konfig::Konfig;
use crate::modell::{Beholder, Kjede, Ml, Ore, Produkt};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Sortering {
    /// Literpris uten pant
    #[default]
    Literpris,
    /// Effektiv pris per beholder
    Pris,
    /// Prosent under 90-dagersmedianen
    Rabatt,
    /// Produktnavn
    Navn,
}

/// Ord i fritekst som slår på sukkerfri-filteret i stedet for å søke på teksten.
const SUKKERFRI_ORD: [&str; 4] = ["sukkerfri", "sukkerfritt", "zero", "sugarfree"];

/// Et ferdig tolket søk.
#[derive(Debug, Clone, PartialEq)]
pub struct Sokefilter {
    /// Normaliserte ord som alle må finnes i produktets navn.
    pub ord: Vec<String>,
    pub merker: Vec<String>,
    pub smaker: Vec<String>,
    pub storrelser: Vec<Ml>,
    pub kjeder: Vec<Kjede>,
    pub sukkerfri: Option<bool>,
    pub beholder: Option<Beholder>,
    pub maks_pris: Option<Ore>,
    pub maks_literpris: Option<Ore>,
    pub sortering: Sortering,
    pub enkeltvis: bool,
    pub alle: bool,
    /// Antall rader som vises uten `--alle`.
    pub antall: usize,
}

impl Sokefilter {
    pub fn fra_args(args: &SokArgs, konfig: &Konfig) -> Self {
        let mut sukkerfri = match (args.sukkerfri, args.med_sukker) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        };
        let mut ord = Vec::new();
        for del in normaliser(&args.sok.join(" ")).split_whitespace() {
            if SUKKERFRI_ORD.contains(&del) {
                sukkerfri.get_or_insert(true);
            } else {
                ord.push(del.to_owned());
            }
        }

        let kjeder = if !args.butikk.is_empty() {
            utvid_kjeder(&args.butikk)
        } else if !konfig.standard_butikker.is_empty() {
            konfig.standard_butikker.clone()
        } else {
            Kjede::ALLE.to_vec()
        };

        Self {
            ord,
            merker: args.merke.iter().map(|m| normaliser(m)).collect(),
            smaker: args.smak.iter().map(|s| normaliser(s)).collect(),
            storrelser: args.storrelse.clone(),
            kjeder,
            sukkerfri,
            beholder: args.beholder,
            maks_pris: args.maks_pris,
            maks_literpris: args.maks_literpris,
            sortering: args.sorter,
            enkeltvis: args.enkeltvis,
            alle: args.alle,
            antall: konfig.standard_antall,
        }
    }

    pub fn kjede_passer(&self, kjede: Kjede) -> bool {
        self.kjeder.contains(&kjede)
    }

    /// Filtrene som kan avgjøres fra katalogen alene (alt unntatt kjede og pris).
    pub fn produkt_passer(&self, produkt: &Produkt) -> bool {
        if !self.merker.is_empty() && !self.merker.contains(&normaliser(&produkt.merke)) {
            return false;
        }
        if !self.smaker.is_empty() {
            let mut smaker = std::iter::once(&produkt.smak).chain(&produkt.smak_alias);
            if !smaker.any(|s| self.smaker.contains(&normaliser(s))) {
                return false;
            }
        }
        if !self.storrelser.is_empty() && !self.storrelser.contains(&produkt.volum_ml) {
            return false;
        }
        if self.sukkerfri.is_some_and(|s| s != produkt.sukkerfri) {
            return false;
        }
        if self.beholder.is_some_and(|b| b != produkt.beholder) {
            return false;
        }
        if self.ord.is_empty() {
            return true;
        }
        // Enkel delstrengsmatching. Uskarp matching (nucleo) kommer i M1.
        let hoystakk = normaliser(&format!(
            "{} {} {} {} {}",
            produkt.navn,
            produkt.merke,
            produkt.linje.as_deref().unwrap_or(""),
            produkt.smak,
            produkt.smak_alias.join(" ")
        ));
        self.ord.iter().all(|ord| hoystakk.contains(ord.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::KjedeValg;
    use crate::katalog::Katalog;

    /// En fast testkatalog, slik at kurateringen av den innebygde katalogen ikke brekker
    /// testene.
    const TESTKATALOG: &str = r#"
        [[produkt]]
        id = "monster-energy-500-boks"
        navn = "Monster Energy"
        merke = "monster"
        smak = "original"
        smak_alias = ["green"]
        sukkerfri = false
        volum_ml = 500
        beholder = "boks"

        [[produkt]]
        id = "monster-ultra-white-500-boks"
        navn = "Monster Ultra White"
        merke = "monster"
        smak = "ultra-white"
        sukkerfri = true
        volum_ml = 500
        beholder = "boks"

        [[produkt]]
        id = "monster-ultra-paradise-500-boks"
        navn = "Monster Ultra Paradise"
        merke = "monster"
        smak = "ultra-paradise"
        sukkerfri = true
        volum_ml = 500
        beholder = "boks"

        [[produkt]]
        id = "monster-mango-loco-500-boks"
        navn = "Monster Mango Loco"
        merke = "monster"
        smak = "mango-loco"
        smak_alias = ["mango"]
        sukkerfri = false
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

        [[produkt]]
        id = "red-bull-sukkerfri-250-boks"
        navn = "Red Bull Sukkerfri"
        merke = "red-bull"
        smak = "original"
        sukkerfri = true
        volum_ml = 250
        beholder = "boks"

        [[produkt]]
        id = "nocco-miami-330-boks"
        navn = "Nocco Miami"
        merke = "nocco"
        smak = "miami"
        sukkerfri = true
        volum_ml = 330
        beholder = "boks"
    "#;

    fn treff(args: SokArgs) -> Vec<String> {
        let filter = Sokefilter::fra_args(&args, &Konfig::default());
        Katalog::fra_toml(TESTKATALOG)
            .unwrap()
            .produkter
            .into_iter()
            .filter(|p| filter.produkt_passer(p))
            .map(|p| p.id.0)
            .collect()
    }

    #[test]
    fn fritekst_og_storrelse() {
        let ider = treff(SokArgs {
            sok: vec!["monster".into(), "ultra".into()],
            storrelse: vec![Ml::new(500).unwrap()],
            ..SokArgs::default()
        });
        assert_eq!(
            ider,
            [
                "monster-ultra-white-500-boks",
                "monster-ultra-paradise-500-boks"
            ]
        );
    }

    #[test]
    fn merke_med_bindestrek_og_mellomrom_er_likt() {
        let a = treff(SokArgs {
            merke: vec!["red-bull".into()],
            ..SokArgs::default()
        });
        let b = treff(SokArgs {
            merke: vec!["Red Bull".into()],
            ..SokArgs::default()
        });
        assert_eq!(a.len(), 2);
        assert_eq!(a, b);
    }

    #[test]
    fn sukkerfri_i_fritekst_blir_filter() {
        let filter = Sokefilter::fra_args(
            &SokArgs {
                sok: vec!["red".into(), "bull".into(), "sukkerfri".into()],
                ..SokArgs::default()
            },
            &Konfig::default(),
        );
        assert_eq!(filter.sukkerfri, Some(true));
        assert_eq!(filter.ord, ["red", "bull"]);
    }

    #[test]
    fn smak_matcher_alias() {
        let ider = treff(SokArgs {
            smak: vec!["mango".into()],
            ..SokArgs::default()
        });
        assert_eq!(ider, ["monster-mango-loco-500-boks"]);
    }

    #[test]
    fn kjeder_fra_flagg_konfig_eller_alle() {
        let mut konfig = Konfig::default();
        let uten = Sokefilter::fra_args(&SokArgs::default(), &konfig);
        assert_eq!(uten.kjeder, Kjede::ALLE);

        konfig.standard_butikker = vec![Kjede::Kiwi];
        let fra_konfig = Sokefilter::fra_args(&SokArgs::default(), &konfig);
        assert_eq!(fra_konfig.kjeder, [Kjede::Kiwi]);

        let args = SokArgs {
            butikk: vec![KjedeValg::En(Kjede::Rema)],
            ..SokArgs::default()
        };
        assert_eq!(Sokefilter::fra_args(&args, &konfig).kjeder, [Kjede::Rema]);
    }
}
