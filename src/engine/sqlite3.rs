//! Wrapper around rusqlite SQLite3 database.
use rusqlite::{Connection, Params, Row, ToSql};

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

/// Wrapper around `rusqlite::Connection`
#[derive(Debug)]
pub struct SQLite3 {
    connection: Connection,
}

impl SQLite3 {
    pub fn new(path: Option<&str>, memory: Option<bool>) -> Self {
        let connection = match (path, memory) {
            (Some(path), None) => Connection::open(path).unwrap(),
            (None, Some(true)) => Connection::open_in_memory().unwrap(),
            _ => panic!("One of `path` or `memory` should be initialized. Not both"),
        };

        Self { connection }
    }

    /// Delete a row
    pub fn delete<Q: Queryable>(&self, data: &Q) -> &Self {
        let mut keys = Q::keys().join(" = ? AND ");
        keys.push_str(" = ?");

        let query = format!("DELETE FROM {} WHERE {}", Q::table(), keys);
        self.connection
            .execute(&query, data.values().as_slice())
            .unwrap();
        self
    }

    /// Insert a row
    pub fn insert<Q: Queryable>(&self, data: &Q) -> &Self {
        let keys = Q::keys();
        let mut vars = "?,".repeat(keys.len());
        vars.pop();
        let keys = keys.join(",");

        let query = format!("INSERT INTO {} ({}) VALUES({})", Q::table(), keys, vars);
        self.connection
            .execute(&query, data.values().as_slice())
            .unwrap();
        self
    }

    /// Fetch entries
    pub fn select<Q: Queryable, P: Params>(
        &self,
        fields: Option<&[&str]>,
        condition_key: Option<&str>,
        condition_param: Option<P>,
    ) -> Vec<Q> {
        let mut out = Vec::new();
        let fields = fields.map_or(String::from("*"), |keys| keys.join(","));

        let mut query = format!("SELECT {} FROM {}", fields, Q::table());
        query
            .push_str(&condition_key.map_or(String::from(""), |key| format!(" WHERE {} = ?", key)));

        let mut smt = self.connection.prepare(&query).unwrap();
        let mut rows = match condition_param {
            Some(arg) => smt.query(arg),
            None => smt.query(()),
        }
        .unwrap();

        while let Some(row) = rows.next().unwrap() {
            out.push(Q::from_row(&row));
        }
        out
    }

    /// Update a row
    pub fn update<Q: Queryable>(
        &self,
        data: &dyn Queryable,
        condition_key: &str,
        condition_param: &str,
    ) -> &Self {
        let mut keys = Q::keys().join(" = ?, ");
        keys.push_str(" = ?");
        let query = format!(
            "UPDATE {} SET {} WHERE {} = \"{}\"",
            Q::table(),
            keys,
            condition_key,
            condition_param
        );
        self.connection
            .execute(&query, data.values().as_slice())
            .unwrap();
        self
    }
}
