//! `databrus konfig …` – kjøres før konfigurasjonen lastes, slik at `init` og `sett`
//! virker også når filen er ugyldig.

use std::fs;

use serde::Serialize;

use crate::cli::{Globale, KonfigKommando};
use crate::feil::{AppFeil, Utgangskode};
use crate::konfig::{self, Konfig, STANDARD_TOML, Stier};
use crate::utdata::json::{self, Konvolutt};

#[derive(Serialize)]
struct KonfigInnhold {
    konfig: Konfig,
}

#[derive(Serialize)]
struct StiInnhold {
    konfig: String,
    database: String,
    katalog: String,
}

pub fn kjor(
    kommando: &KonfigKommando,
    stier: &Stier,
    globale: &Globale,
) -> Result<Utgangskode, AppFeil> {
    match kommando {
        KonfigKommando::Vis => {
            let konfig = Konfig::last(&stier.konfigfil)?.sensurert();
            if !stier.konfigfil.exists() {
                eprintln!(
                    "(ingen konfigurasjonsfil på {} – viser standardverdiene)",
                    stier.konfigfil.display()
                );
            }
            if globale.json {
                json::skriv(&Konvolutt::ny(KonfigInnhold { konfig }))?;
            } else {
                let tekst = toml::to_string_pretty(&konfig)
                    .map_err(|feil| AppFeil::Konfig(feil.to_string()))?;
                print!("{tekst}");
            }
        }
        KonfigKommando::Sti => {
            let stier = StiInnhold {
                konfig: stier.konfigfil.display().to_string(),
                database: stier.database().display().to_string(),
                katalog: stier.katalog_overstyring().display().to_string(),
            };
            if globale.json {
                json::skriv(&Konvolutt::ny(stier))?;
            } else {
                println!("konfigurasjon  {}", stier.konfig);
                println!("database       {}", stier.database);
                println!("katalog        {}", stier.katalog);
            }
        }
        KonfigKommando::Sett { nokkel, verdi } => {
            konfig::sett_verdi(&stier.konfigfil, nokkel, verdi)?;
            // Verdien skrives ikke ut – den kan være en API-nøkkel.
            eprintln!("satte {nokkel} i {}", stier.konfigfil.display());
        }
        KonfigKommando::Init { tving } => {
            if stier.konfigfil.exists() && !tving {
                return Err(AppFeil::Bruk(format!(
                    "{} finnes allerede – bruk --tving for å overskrive",
                    stier.konfigfil.display()
                )));
            }
            if let Some(mappe) = stier.konfigfil.parent() {
                fs::create_dir_all(mappe)?;
            }
            fs::write(&stier.konfigfil, STANDARD_TOML)?;
            eprintln!("skrev {}", stier.konfigfil.display());
        }
    }
    Ok(Utgangskode::Ok)
}
