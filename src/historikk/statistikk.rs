//! Referansepriser fra historikken: L30, M90 og ATL (SPEC §7.6). Beregningen kommer i M2.

use jiff::Timestamp;

use crate::modell::Ore;

/// Referanseverdiene for én (produkt, kjede).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Referanser {
    /// Laveste pris de 30 dagene før gjeldende pris begynte (Omnibus-prinsippet).
    pub l30: Option<Ore>,
    /// Tidsvektet median siste 90 dager, uten gjeldende intervall.
    pub m90: Option<Ore>,
    /// Laveste pris i hele den lokale historikken.
    pub atl: Option<Ore>,
    /// Dager med kjent pris de siste 90 dagene.
    pub dekning_dager: u32,
}

/// Ett intervall der literprisen var kjent. Hull i historikken er ukjente, ikke
/// «samme pris som sist» (SPEC §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Intervall {
    pub literpris: Ore,
    pub fra: Timestamp,
    pub til: Timestamp,
}

/// Beregner referanseverdiene. Til M2 er implementert, finnes ingen dekning.
pub fn beregn(_intervaller: &[Intervall], _na: Timestamp) -> Referanser {
    Referanser::default()
}
