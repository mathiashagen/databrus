//! Konfigurasjon (SPEC §10) og filstier (SPEC §7.1).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::feil::AppFeil;
use crate::historikk::Terskler;
use crate::modell::{KildeId, Kjede, Medlemsprogram};
use crate::prising::PantKonfig;

/// Overstyrer datamappen (brukes av tester og for bærbare oppsett).
pub const ENV_DATAMAPPE: &str = "DATABRUS_DATA_DIR";
/// Kassalapp-nøkkelen i miljøet går foran konfigurasjonsfilen (SPEC §10.2).
pub const ENV_API_NOKKEL: &str = "KASSALAPP_API_KEY";
/// Gulvet for `henting.min_intervall_minutter` (SPEC §7.4).
pub const MIN_INTERVALL_GULV: u32 = 15;

/// Den kommenterte filen `konfig init` skriver. Må tolkes til [`Konfig::default`].
pub const STANDARD_TOML: &str = r#"# databrus – konfigurasjon (se SPEC.md §10)

# Lojalitetsprogrammer du er med i. Medlemspriser brukes i rangeringen
# bare for disse: coop | trumf | ae | kiwi-pluss
medlemskap = []

# Kjeder som brukes når --butikk ikke er gitt. Tom liste betyr alle kjeder.
standard_butikker = []

# Antall rader i tabellen når --alle ikke er gitt.
standard_antall = 20

# Farger: auto | alltid | aldri
farge = "auto"

[henting]
# Hvor lenge data fra en kilde regnes som ferske.
ttl_timer = 6
# Minste tid mellom to hentinger fra samme kilde. Kan ikke settes lavere enn 15.
min_intervall_minutter = 15

[pant]
liten_ore = 200   # beholdere til og med grense_ml
stor_ore = 300    # beholdere over grense_ml
grense_ml = 500

[vurdering]
min_dekning_dager = 14
supert_under_median_prosent = 20
bra_under_median_prosent = 10
atl_toleranse_prosent = 2
lureri_prisokning_prosent = 5
prisfall_prosent = 10

[kilder.kassalapp]
aktiv = true
# Gratis nøkkel fra https://kassal.app/api. Miljøvariabelen KASSALAPP_API_KEY går foran.
api_nokkel = ""

[kilder.oda]
aktiv = true

[kilder.rema]
aktiv = true

[kilder.coop]
aktiv = true
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fargevalg {
    #[default]
    Auto,
    Alltid,
    Aldri,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Konfig {
    pub medlemskap: Vec<Medlemsprogram>,
    pub standard_butikker: Vec<Kjede>,
    pub standard_antall: usize,
    pub farge: Fargevalg,
    pub henting: Henting,
    pub pant: PantKonfig,
    pub vurdering: Terskler,
    pub kilder: Kilder,
}

impl Default for Konfig {
    fn default() -> Self {
        Self {
            medlemskap: Vec::new(),
            standard_butikker: Vec::new(),
            standard_antall: 20,
            farge: Fargevalg::Auto,
            henting: Henting::default(),
            pant: PantKonfig::default(),
            vurdering: Terskler::default(),
            kilder: Kilder::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Henting {
    pub ttl_timer: u32,
    pub min_intervall_minutter: u32,
}

impl Default for Henting {
    fn default() -> Self {
        Self {
            ttl_timer: 6,
            min_intervall_minutter: MIN_INTERVALL_GULV,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Kilder {
    pub kassalapp: KassalappKonfig,
    pub oda: KildeKonfig,
    pub rema: KildeKonfig,
    pub coop: KildeKonfig,
}

impl Kilder {
    pub fn er_aktiv(&self, kilde: KildeId) -> bool {
        match kilde {
            KildeId::Kassalapp => self.kassalapp.aktiv,
            KildeId::Oda => self.oda.aktiv,
            KildeId::Rema => self.rema.aktiv,
            KildeId::Coop => self.coop.aktiv,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KildeKonfig {
    pub aktiv: bool,
}

impl Default for KildeKonfig {
    fn default() -> Self {
        Self { aktiv: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KassalappKonfig {
    pub aktiv: bool,
    pub api_nokkel: String,
}

impl Default for KassalappKonfig {
    fn default() -> Self {
        Self {
            aktiv: true,
            api_nokkel: String::new(),
        }
    }
}

impl Konfig {
    /// Leser konfigurasjonen. En manglende fil gir standardverdier.
    pub fn last(sti: &Path) -> Result<Self, AppFeil> {
        let tekst = match fs::read_to_string(sti) {
            Ok(tekst) => tekst,
            Err(feil) if feil.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(feil) => return Err(feil.into()),
        };
        let konfig: Konfig = toml::from_str(&tekst)
            .map_err(|feil| AppFeil::Konfig(format!("{}: {feil}", sti.display())))?;
        konfig.valider()?;
        Ok(konfig)
    }

    pub fn valider(&self) -> Result<(), AppFeil> {
        if self.henting.min_intervall_minutter < MIN_INTERVALL_GULV {
            return Err(AppFeil::Konfig(format!(
                "henting.min_intervall_minutter må være minst {MIN_INTERVALL_GULV}"
            )));
        }
        if self.henting.ttl_timer == 0 {
            return Err(AppFeil::Konfig("henting.ttl_timer må være minst 1".into()));
        }
        if self.standard_antall == 0 {
            return Err(AppFeil::Konfig("standard_antall må være minst 1".into()));
        }
        if self.pant.grense_ml == 0 || self.pant.liten_ore.0 < 0 || self.pant.stor_ore.0 < 0 {
            return Err(AppFeil::Konfig("ugyldige pantesatser".into()));
        }
        Ok(())
    }

    /// Kassalapp-nøkkelen: miljøvariabelen først, så konfigurasjonsfilen.
    pub fn api_nokkel(&self) -> Option<String> {
        std::env::var(ENV_API_NOKKEL)
            .ok()
            .filter(|n| !n.trim().is_empty())
            .or_else(|| {
                let nokkel = self.kilder.kassalapp.api_nokkel.trim();
                (!nokkel.is_empty()).then(|| nokkel.to_owned())
            })
    }

    /// En kopi som trygt kan vises: API-nøkkelen er skjult.
    pub fn sensurert(&self) -> Self {
        let mut kopi = self.clone();
        if !kopi.kilder.kassalapp.api_nokkel.is_empty() {
            kopi.kilder.kassalapp.api_nokkel = "***".into();
        }
        kopi
    }
}

/// Setter én verdi i konfigurasjonsfilen, f.eks. `henting.ttl_timer = 12`.
///
/// Nøkkelen må finnes i skjemaet, og resultatet må validere. Merk: filen skrives på nytt
/// uten kommentarer.
pub fn sett_verdi(sti: &Path, nokkel: &str, verdi: &str) -> Result<(), AppFeil> {
    let standard = toml::Table::try_from(Konfig::default())
        .map_err(|feil| AppFeil::Konfig(feil.to_string()))?;
    if !nokkel_finnes(&standard, nokkel) {
        return Err(AppFeil::Konfig(format!("ukjent nøkkel «{nokkel}»")));
    }

    let mut tabell: toml::Table = match fs::read_to_string(sti) {
        Ok(tekst) => toml::from_str(&tekst)
            .map_err(|feil| AppFeil::Konfig(format!("{}: {feil}", sti.display())))?,
        Err(feil) if feil.kind() == io::ErrorKind::NotFound => toml::Table::new(),
        Err(feil) => return Err(feil.into()),
    };
    sett_i_tabell(&mut tabell, nokkel, tolk_verdi(verdi));

    let konfig: Konfig = toml::Value::Table(tabell.clone())
        .try_into()
        .map_err(|feil| AppFeil::Konfig(format!("ugyldig verdi for {nokkel}: {feil}")))?;
    konfig.valider()?;

    if let Some(mappe) = sti.parent() {
        fs::create_dir_all(mappe)?;
    }
    let tekst =
        toml::to_string_pretty(&tabell).map_err(|feil| AppFeil::Konfig(feil.to_string()))?;
    fs::write(sti, tekst)?;
    Ok(())
}

fn nokkel_finnes(tabell: &toml::Table, nokkel: &str) -> bool {
    let mut gjeldende = tabell;
    let mut deler = nokkel.split('.').peekable();
    while let Some(del) = deler.next() {
        match (gjeldende.get(del), deler.peek()) {
            (Some(_), None) => return true,
            (Some(toml::Value::Table(under)), Some(_)) => gjeldende = under,
            _ => return false,
        }
    }
    false
}

/// Tolker verdien som TOML (`12`, `true`, `["coop"]`), ellers som en streng.
fn tolk_verdi(verdi: &str) -> toml::Value {
    toml::from_str::<toml::Table>(&format!("v = {verdi}"))
        .ok()
        .and_then(|mut t| t.remove("v"))
        .unwrap_or_else(|| toml::Value::String(verdi.to_owned()))
}

fn sett_i_tabell(tabell: &mut toml::Table, nokkel: &str, verdi: toml::Value) {
    match nokkel.split_once('.') {
        None => {
            tabell.insert(nokkel.to_owned(), verdi);
        }
        Some((hode, resten)) => {
            let under = tabell
                .entry(hode.to_owned())
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if !under.is_table() {
                *under = toml::Value::Table(toml::Table::new());
            }
            if let toml::Value::Table(under) = under {
                sett_i_tabell(under, resten, verdi);
            }
        }
    }
}

/// Hvor filene ligger (SPEC §7.1).
#[derive(Debug, Clone)]
pub struct Stier {
    pub konfigfil: PathBuf,
    pub datamappe: PathBuf,
}

impl Stier {
    /// `konfigfil` kommer fra `--konfig` / `DATABRUS_KONFIG` når de er satt.
    pub fn finn(konfigfil: Option<&Path>) -> Result<Self, AppFeil> {
        let baser = BaseDirs::new();
        let konfigfil = match (konfigfil, &baser) {
            (Some(sti), _) => sti.to_path_buf(),
            (None, Some(b)) => b.config_dir().join("databrus").join("konfig.toml"),
            (None, None) => return Err(ingen_hjemmemappe()),
        };
        let datamappe = match (std::env::var_os(ENV_DATAMAPPE), &baser) {
            (Some(sti), _) if !sti.is_empty() => PathBuf::from(sti),
            (_, Some(b)) => b.data_local_dir().join("databrus"),
            (_, None) => return Err(ingen_hjemmemappe()),
        };
        Ok(Self {
            konfigfil,
            datamappe,
        })
    }

    pub fn database(&self) -> PathBuf {
        self.datamappe.join("databrus.sqlite")
    }

    /// Brukerens katalogoverstyring ligger ved siden av konfigurasjonsfilen.
    pub fn katalog_overstyring(&self) -> PathBuf {
        self.konfigfil
            .parent()
            .map_or_else(|| PathBuf::from("katalog.toml"), |m| m.join("katalog.toml"))
    }
}

fn ingen_hjemmemappe() -> AppFeil {
    AppFeil::Konfig(format!(
        "fant ingen hjemmemappe – bruk --konfig og {ENV_DATAMAPPE}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standardfilen_gir_standardverdiene() {
        let konfig: Konfig = toml::from_str(STANDARD_TOML).unwrap();
        assert_eq!(konfig, Konfig::default());
        konfig.valider().unwrap();
    }

    #[test]
    fn tom_fil_gir_standardverdiene() {
        let konfig: Konfig = toml::from_str("").unwrap();
        assert_eq!(konfig, Konfig::default());
    }

    #[test]
    fn min_intervall_under_gulvet_avvises() {
        let konfig: Konfig = toml::from_str("[henting]\nmin_intervall_minutter = 5").unwrap();
        assert!(konfig.valider().is_err());
    }

    #[test]
    fn verdier_fra_spec_tolkes() {
        let konfig: Konfig = toml::from_str(
            "medlemskap = [\"coop\", \"kiwi-pluss\"]\nstandard_butikker = [\"coop-extra\"]",
        )
        .unwrap();
        assert_eq!(
            konfig.medlemskap,
            [Medlemsprogram::Coop, Medlemsprogram::KiwiPluss]
        );
        assert_eq!(konfig.standard_butikker, [Kjede::CoopExtra]);
    }

    #[test]
    fn sett_verdi_skriver_og_validerer() {
        let mappe = tempfile::tempdir().unwrap();
        let sti = mappe.path().join("konfig.toml");

        sett_verdi(&sti, "henting.ttl_timer", "12").unwrap();
        sett_verdi(&sti, "medlemskap", "[\"trumf\"]").unwrap();
        let konfig = Konfig::last(&sti).unwrap();
        assert_eq!(konfig.henting.ttl_timer, 12);
        assert_eq!(konfig.medlemskap, [Medlemsprogram::Trumf]);

        assert!(sett_verdi(&sti, "henting.min_intervall_minutter", "5").is_err());
        assert!(sett_verdi(&sti, "finnes.ikke", "1").is_err());
        assert!(sett_verdi(&sti, "farge", "rosa").is_err());
        // En avvist verdi skal ikke ha endret filen.
        assert_eq!(
            Konfig::last(&sti).unwrap().henting.min_intervall_minutter,
            15
        );
    }

    #[test]
    fn sensurert_skjuler_api_nokkel() {
        let mut konfig = Konfig::default();
        konfig.kilder.kassalapp.api_nokkel = "hemmelig".into();
        assert_eq!(konfig.sensurert().kilder.kassalapp.api_nokkel, "***");
    }
}
