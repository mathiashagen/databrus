//! Schema migrations. New migrations are appended – existing ones are never changed
//! (the one exception being the pre-release rename, see `reject_prerelease_schema`).

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(include_str!(
        "../../migrations/001_initial_schema.sql"
    ))])
}

pub fn migrate(conn: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    migrations().to_latest(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        migrations().validate().unwrap();
    }
}
