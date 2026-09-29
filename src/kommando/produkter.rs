//! `databrus produkter` – produktkatalogen, filtrert, uten priser.

use serde::Serialize;

use crate::Kontekst;
use crate::cli::ProdukterArgs;
use crate::feil::{AppFeil, Utgangskode};
use crate::lager::Umatchet;
use crate::modell::Produkt;
use crate::sok::Sokefilter;
use crate::utdata::json::{self, Hode, Konvolutt};
use crate::utdata::{Utdataformat, format, tabell};

#[derive(Serialize)]
struct Innhold<'a> {
    produkter: Vec<&'a Produkt>,
}

#[derive(Serialize)]
struct UkjentInnhold {
    ukjente: Vec<Umatchet>,
}

pub fn kjor(args: &ProdukterArgs, ktx: &Kontekst) -> Result<Utgangskode, AppFeil> {
    if args.ukjente {
        return ukjente(ktx);
    }

    let katalog = ktx.katalog()?;
    let filter = Sokefilter::fra_args(&args.filtre, &ktx.konfig);
    let mut produkter: Vec<&Produkt> = katalog
        .produkter
        .iter()
        .filter(|p| filter.produkt_passer(p))
        .collect();
    produkter.sort_by(|a, b| a.navn.cmp(&b.navn).then(a.volum_ml.cmp(&b.volum_ml)));

    match ktx.utdataformat() {
        Utdataformat::Json => json::skriv(&Konvolutt::ny(Innhold { produkter }))?,
        Utdataformat::JsonLinjer => json::skriv_linjer(&Hode::ny(), &produkter)?,
        Utdataformat::Tabell if produkter.is_empty() => {
            eprintln!("ingen produkter passer filtrene");
        }
        Utdataformat::Tabell => {
            let mut t = tabell::ny(&["Produkt", "Str.", "Beholder", "Sukkerfri", "EAN-er", "Id"]);
            for p in &produkter {
                t.add_row(vec![
                    p.navn.clone(),
                    format::liter(p.volum_ml),
                    p.beholder.slug().to_owned(),
                    if p.sukkerfri { "ja" } else { "nei" }.to_owned(),
                    p.gtin.len().to_string(),
                    p.id.0.clone(),
                ]);
            }
            tabell::skriv(&t)?;
        }
    }
    Ok(Utgangskode::Ok)
}

fn ukjente(ktx: &Kontekst) -> Result<Utgangskode, AppFeil> {
    let lager = ktx.apne_lager()?;
    let ukjente = lager.umatchede_oppforinger()?;
    match ktx.utdataformat() {
        Utdataformat::Json => json::skriv(&Konvolutt::ny(UkjentInnhold { ukjente }))?,
        Utdataformat::JsonLinjer => json::skriv_linjer(&Hode::ny(), &ukjente)?,
        Utdataformat::Tabell if ukjente.is_empty() => {
            eprintln!("ingen umatchede oppføringer");
        }
        Utdataformat::Tabell => {
            let mut t = tabell::ny(&["Navn", "Kjede", "Kilde", "EAN"]);
            for u in &ukjente {
                t.add_row(vec![
                    u.raanavn.clone(),
                    u.kjede.clone(),
                    u.kilde.clone(),
                    u.gtin.clone().unwrap_or_else(|| "–".into()),
                ]);
            }
            tabell::skriv(&t)?;
        }
    }
    Ok(Utgangskode::Ok)
}
