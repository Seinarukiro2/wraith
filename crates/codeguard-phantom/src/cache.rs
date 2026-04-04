use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct PypiCache {
    conn: Connection,
    ttl_secs: u64,
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub package_name: String,
    pub status: u16,
    pub response: Option<String>,
    pub fetched_at: u64,
}

impl PypiCache {
    pub fn open(path: &Path, ttl_secs: u64) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS pypi_cache (
                package_name TEXT PRIMARY KEY,
                status       INTEGER NOT NULL,
                response     TEXT,
                fetched_at   INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_pypi_fetched ON pypi_cache(fetched_at);",
        )?;
        Ok(Self { conn, ttl_secs })
    }

    pub fn get(&self, package: &str) -> Option<CacheEntry> {
        let now = current_epoch();
        let mut stmt = self
            .conn
            .prepare_cached(
                "SELECT package_name, status, response, fetched_at
                 FROM pypi_cache WHERE package_name = ?1 AND fetched_at > ?2",
            )
            .ok()?;
        let cutoff = now.saturating_sub(self.ttl_secs);
        stmt.query_row(rusqlite::params![package, cutoff], |row| {
            Ok(CacheEntry {
                package_name: row.get(0)?,
                status: row.get(1)?,
                response: row.get(2)?,
                fetched_at: row.get(3)?,
            })
        })
        .ok()
    }

    pub fn put(&self, package: &str, status: u16, response: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO pypi_cache (package_name, status, response, fetched_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![package, status, response, current_epoch()],
        )?;
        Ok(())
    }

    pub fn cleanup_expired(&self) -> Result<()> {
        let cutoff = current_epoch().saturating_sub(self.ttl_secs);
        self.conn.execute(
            "DELETE FROM pypi_cache WHERE fetched_at < ?1",
            rusqlite::params![cutoff],
        )?;
        Ok(())
    }
}

fn current_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
