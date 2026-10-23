//! Local-first transcription history backed by SQLite.
//!
//! Stored at `<app_data_dir>/history.db`. Each dictation is persisted on
//! completion; the table is pruned to the configured `history_limit`.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub text: String,
    pub duration_ms: u64,
    pub engine: String,
    pub timestamp: u64,
    pub favorite: bool,
    /// Time the model took to transcribe this clip (0 = not recorded).
    #[serde(default)]
    pub latency_ms: u64,
}

/// Real-world latency of one model on this machine, from History.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStats {
    pub model_id: String,
    pub count: u64,
    pub avg_latency_ms: u64,
    /// Average transcription time per second of audio.
    pub ms_per_audio_second: u64,
}

/// Thread-safe history store (Connection is Send but not Sync → Mutex).
pub struct History {
    conn: Mutex<Connection>,
}

impl History {
    pub fn open(db_path: &Path) -> Result<Self, String> {
        let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS dictations (
                id          TEXT PRIMARY KEY,
                text        TEXT NOT NULL,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                engine      TEXT NOT NULL DEFAULT '',
                timestamp   INTEGER NOT NULL DEFAULT 0,
                favorite    INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_dictations_ts ON dictations(timestamp DESC);",
        )
        .map_err(|e| e.to_string())?;
        // v0.3.18 migration: per-dictation model latency. Fails harmlessly
        // ("duplicate column") once the column exists.
        let _ = conn.execute(
            "ALTER TABLE dictations ADD COLUMN latency_ms INTEGER NOT NULL DEFAULT 0",
            [],
        );
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn insert(&self, entry: &HistoryEntry) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO dictations (id, text, duration_ms, engine, timestamp, favorite, latency_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                entry.id,
                entry.text,
                entry.duration_ms as i64,
                entry.engine,
                entry.timestamp as i64,
                entry.favorite as i64,
                entry.latency_ms as i64,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list(&self, limit: u32) -> Result<Vec<HistoryEntry>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, text, duration_ms, engine, timestamp, favorite, latency_ms
                 FROM dictations ORDER BY timestamp DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([limit as i64], |r| {
                Ok(HistoryEntry {
                    id: r.get(0)?,
                    text: r.get(1)?,
                    duration_ms: r.get::<_, i64>(2)? as u64,
                    engine: r.get(3)?,
                    timestamp: r.get::<_, i64>(4)? as u64,
                    favorite: r.get::<_, i64>(5)? != 0,
                    latency_ms: r.get::<_, i64>(6)? as u64,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// Average measured latency per model over the most recent 50 timed
    /// dictations of each model.
    pub fn model_stats(&self) -> Result<Vec<ModelStats>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT engine, COUNT(*), AVG(latency_ms), SUM(latency_ms), SUM(duration_ms)
                 FROM (
                    SELECT engine, latency_ms, duration_ms,
                           ROW_NUMBER() OVER (PARTITION BY engine ORDER BY timestamp DESC) AS rn
                    FROM dictations WHERE latency_ms > 0 AND engine != ''
                 ) WHERE rn <= 50 GROUP BY engine",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                let lat_sum: i64 = r.get(3)?;
                let dur_sum: i64 = r.get(4)?;
                Ok(ModelStats {
                    model_id: r.get(0)?,
                    count: r.get::<_, i64>(1)? as u64,
                    avg_latency_ms: r.get::<_, f64>(2)?.round() as u64,
                    ms_per_audio_second: if dur_sum > 0 {
                        (lat_sum as f64 * 1000.0 / dur_sum as f64).round() as u64
                    } else {
                        0
                    },
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM dictations WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM dictations", []).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Keep only the newest `limit` entries (0 = unlimited).
    pub fn prune(&self, limit: u32) -> Result<(), String> {
        if limit == 0 {
            return Ok(());
        }
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM dictations WHERE id NOT IN (
                SELECT id FROM dictations ORDER BY timestamp DESC LIMIT ?1
             )",
            [limit as i64],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
