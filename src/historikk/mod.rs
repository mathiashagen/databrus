//! Prishistorikk: referansepriser, vurdering og tilbudsdeteksjon (SPEC §7.6–7.9).

pub mod deteksjon;
pub mod statistikk;
pub mod vurdering;

pub use statistikk::{Intervall, Referanser};
pub use vurdering::{Terskler, vurder};
