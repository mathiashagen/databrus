//! Skjemamigreringer. Nye migreringer legges til sist – eksisterende endres aldri.

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

pub fn migreringer() -> Migrations<'static> {
    Migrations::new(vec![M::up(include_str!("../../migrations/001_init.sql"))])
}

pub fn migrer(conn: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    migreringer().to_latest(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migreringene_er_gyldige() {
        migreringer().validate().unwrap();
    }
}
