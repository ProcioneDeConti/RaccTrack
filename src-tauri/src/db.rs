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

-- v2: added updated_at (hexdb record age). Cache table — safe to rebuild.
DROP TABLE IF EXISTS route_cache;
CREATE TABLE IF NOT EXISTS route_cache (
    callsign   TEXT PRIMARY KEY,
    origin     TEXT,
    dest       TEXT,
    updated_at INTEGER,
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

-- Generic JSON cache (weather, airspace, chart index, ...). key = "<ns>:<params>".
CREATE TABLE IF NOT EXISTS kv_cache (
    key        TEXT PRIMARY KEY,
    json       TEXT NOT NULL,
    fetched_at INTEGER NOT NULL
);

-- Geofences: feature removed for now (rendering/eval issues); table kept so the
-- data survives if it's brought back. See git history for src/geofence.rs.
CREATE TABLE IF NOT EXISTS geofences (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    label     TEXT NOT NULL,
    json      TEXT NOT NULL,
    enabled   INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS chart_pdf (
    url        TEXT PRIMARY KEY,
    data       BLOB NOT NULL,
    bytes      INTEGER NOT NULL,
    fetched_at INTEGER NOT NULL,
    last_used  INTEGER NOT NULL
);

-- Self-collected flight history: notable state changes for aircraft we've had
-- in view (squawk / emergency / callsign / takeoff / landing) plus watch and
-- NA-wide emergency-squawk hits. Pruned by age (history_retention_days).
CREATE TABLE IF NOT EXISTS aircraft_events (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    hex      TEXT NOT NULL,
    at       INTEGER NOT NULL,
    kind     TEXT NOT NULL,
    flight   TEXT,
    from_val TEXT,
    to_val   TEXT,
    lat      REAL,
    lon      REAL
);
CREATE INDEX IF NOT EXISTS idx_events_at ON aircraft_events(at DESC);
CREATE INDEX IF NOT EXISTS idx_events_hex ON aircraft_events(hex, at DESC);

-- Spotter logbook: one row per airframe the user has had in view, ever.
-- `count` is distinct appearances (a gap of > 1 h starts a new one).
CREATE TABLE IF NOT EXISTS sightings (
    hex          TEXT PRIMARY KEY,
    first_seen   INTEGER NOT NULL,
    last_seen    INTEGER NOT NULL,
    count        INTEGER NOT NULL DEFAULT 1,
    flight       TEXT,
    registration TEXT,
    type_code    TEXT,
    description  TEXT,
    military     INTEGER NOT NULL DEFAULT 0,
    note         TEXT
);
CREATE INDEX IF NOT EXISTS idx_sightings_last ON sightings(last_seen DESC);
"#;

/// One-off column additions to tables that already existed before the column
/// was introduced — `CREATE TABLE IF NOT EXISTS` above only helps on a brand
/// new database; an existing install's `sightings` table needs an explicit,
/// idempotent `ALTER TABLE` to pick up anything added later.
fn migrate(conn: &Connection) -> Result<()> {
    let has_column: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('sightings') WHERE name = 'first_seen_direct'")?
        .exists([])?;
    if !has_column {
        conn.execute(
            "ALTER TABLE sightings ADD COLUMN first_seen_direct INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    Ok(())
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
        )?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
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

    // --- generic JSON cache (kv_cache) ---

    /// Returns the cached JSON string if present and younger than `ttl_ms`.
    pub fn kv_get(&self, key: &str, ttl_ms: i64) -> Result<Option<String>> {
        self.with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT json, fetched_at FROM kv_cache WHERE key = ?1")?;
            let mut rows = stmt.query([key])?;
            if let Some(r) = rows.next()? {
                let json: String = r.get(0)?;
                let fetched_at: i64 = r.get(1)?;
                if crate::util::now_ms() - fetched_at < ttl_ms {
                    return Ok(Some(json));
                }
            }
            Ok(None)
        })
    }

    /// Cached JSON regardless of age (for serving stale on fetch failure).
    pub fn kv_get_stale(&self, key: &str) -> Result<Option<String>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare("SELECT json FROM kv_cache WHERE key = ?1")?;
            let mut rows = stmt.query([key])?;
            Ok(rows.next()?.map(|r| r.get::<_, String>(0)).transpose()?)
        })
    }

    pub fn kv_put(&self, key: &str, json: &str) -> Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT INTO kv_cache(key, json, fetched_at) VALUES(?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET json = excluded.json, fetched_at = excluded.fetched_at",
                rusqlite::params![key, json, crate::util::now_ms()],
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
