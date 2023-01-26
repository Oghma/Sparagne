//! Wrapper around rusqlite SQLite3 database.
use rusqlite::Row;

pub trait Queryable {
    fn table() -> &'static str
    where
        Self: Sized;
    fn keys() -> Vec<&'static str>
    where
        Self: Sized;
    fn values(&self) -> Vec<&dyn ToSql>;
    fn from_row(row: &Row) -> Self
    where
        Self: Sized;
}
