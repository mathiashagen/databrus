//! Vurdering av en pris mot historikken (SPEC §7.6). Reglene kommer i M2.

use serde::{Deserialize, Serialize};

use super::Referanser;
use crate::modell::{Ore, Vurdering};

/// Terskler for vurderingen, fra `[vurdering]` i konfigurasjonen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Terskler {
    pub min_dekning_dager: u32,
    pub supert_under_median_prosent: u32,
    pub bra_under_median_prosent: u32,
    pub atl_toleranse_prosent: u32,
    pub lureri_prisokning_prosent: u32,
    pub prisfall_prosent: u32,
}

impl Default for Terskler {
    fn default() -> Self {
        Self {
            min_dekning_dager: 14,
            supert_under_median_prosent: 20,
            bra_under_median_prosent: 10,
            atl_toleranse_prosent: 2,
            lureri_prisokning_prosent: 5,
            prisfall_prosent: 10,
        }
    }
}

/// Vurderer `literpris` mot referansene. Til M2 er alt `UKJENT`.
pub fn vurder(
    _literpris: Ore,
    _er_tilbud: bool,
    _pris_for_tilbudet: Option<Ore>,
    _referanser: &Referanser,
    _terskler: &Terskler,
) -> Vurdering {
    Vurdering::Ukjent
}
