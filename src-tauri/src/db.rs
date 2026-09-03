//! SQLite-backed persistence: settings, watchlist, enrichment caches, tile cache.

use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::Path;

pub struct Db {
    conn: Mutex<Connection>,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    k TEXT PRIMARY KEY,
    v TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS watchlist (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    kind    TEXT NOT NULL,
    value   TEXT NOT NULL,
    label   TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    UNIQUE(kind, value)
);

CREATE TABLE IF NOT EXISTS route_cache (
    callsign   TEXT PRIMARY KEY,
    origin     TEXT,
    dest       TEXT,
    fetched_at INTEGER NOT NULL
);

DROP TABLE IF EXISTS photo_cache;
CREATE TABLE IF NOT EXISTS image_cache (
    hex        TEXT PRIMARY KEY,
    json       TEXT NOT NULL,
    fetched_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS hexdb_cache (
    hex        TEXT PRIMARY KEY,
    json       TEXT NOT NULL,
    fetched_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS geocode_cache (
    query      TEXT PRIMARY KEY,
    json       TEXT NOT NULL,
    fetched_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tiles (
    path         TEXT PRIMARY KEY,
    data         BLOB NOT NULL,
    content_type TEXT NOT NULL,
    bytes        INTEGER NOT NULL,
    fetched_at   INTEGER NOT NULL,
    last_used    INTEGER NOT NULL
);
"#;

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
        )?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = self.conn.lock();
        f(&guard)
    }

    // --- settings ---

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare("SELECT v FROM settings WHERE k = ?1")?;
            let mut rows = stmt.query([key])?;
            Ok(rows.next()?.map(|r| r.get::<_, String>(0)).transpose()?)
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT INTO settings(k, v) VALUES(?1, ?2)
                 ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                (key, value),
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.get_setting("x").unwrap(), None);
        db.set_setting("x", "1").unwrap();
        db.set_setting("x", "2").unwrap();
        assert_eq!(db.get_setting("x").unwrap().as_deref(), Some("2"));
    }
}
