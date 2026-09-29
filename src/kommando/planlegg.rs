//! `databrus planlegg …` – planlagt daglig henting.

use crate::Kontekst;
use crate::cli::PlanleggKommando;
use crate::feil::{AppFeil, Utgangskode};
use crate::planlegg;

pub fn kjor(kommando: &PlanleggKommando, _ktx: &Kontekst) -> Result<Utgangskode, AppFeil> {
    let planlegger = planlegg::for_plattform();
    match kommando {
        PlanleggKommando::Installer { tid } => {
            planlegger.installer(*tid)?;
            println!(
                "installerte {} kl. {}",
                planlegg::OPPGAVENAVN,
                tid.strftime("%H:%M")
            );
        }
        PlanleggKommando::Fjern => {
            planlegger.fjern()?;
            println!("fjernet {}", planlegg::OPPGAVENAVN);
        }
        PlanleggKommando::Status => match planlegger.status()? {
            Some(beskrivelse) => println!("{beskrivelse}"),
            None => println!("ingen planlagt henting er installert"),
        },
    }
    Ok(Utgangskode::Ok)
}
