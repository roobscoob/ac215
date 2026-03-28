use rusqlite::{Connection, params};
use serde::Serialize;
use std::sync::Mutex;

/// Shared local SQLite database for proxy state.
pub struct LocalDb {
    conn: Mutex<Connection>,
}

// ── Network override ────────────────────────────────────────────────────────

/// The original network address we overwrote in tblNetworks.
#[derive(Debug, Clone)]
pub struct NetworkOverride {
    pub network_id: i64,
    pub original_ip1: u8,
    pub original_ip2: u8,
    pub original_ip3: u8,
    pub original_ip4: u8,
    pub original_port: i32,
}

// ── Webhooks ───────��────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Webhook {
    pub id: String,
    pub url: String,
    #[serde(skip_serializing)]
    pub signing_key: String,
}

impl LocalDb {
    pub fn open(path: &str) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS webhooks (
                id          TEXT PRIMARY KEY,
                url         TEXT NOT NULL,
                signing_key TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS network_override (
                network_id    INTEGER PRIMARY KEY,
                original_ip1  INTEGER NOT NULL,
                original_ip2  INTEGER NOT NULL,
                original_ip3  INTEGER NOT NULL,
                original_ip4  INTEGER NOT NULL,
                original_port INTEGER NOT NULL
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ��─ Webhook operations ────��─────────────────────────────────────────

    pub fn insert_webhook(
        &self,
        id: &str,
        url: &str,
        signing_key: &str,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO webhooks (id, url, signing_key) VALUES (?1, ?2, ?3)",
            params![id, url, signing_key],
        )?;
        Ok(())
    }

    pub fn list_webhooks(&self) -> Result<Vec<Webhook>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, url, signing_key FROM webhooks")?;
        let rows = stmt.query_map([], |row| {
            Ok(Webhook {
                id: row.get(0)?,
                url: row.get(1)?,
                signing_key: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_webhook(&self, id: &str) -> Result<Option<Webhook>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, url, signing_key FROM webhooks WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(Webhook {
                id: row.get(0)?,
                url: row.get(1)?,
                signing_key: row.get(2)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn delete_webhook(&self, id: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute("DELETE FROM webhooks WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    // ── Network override operations ───────────────────────────────────────

    /// Save the original network address before overwriting it.
    pub fn save_override(&self, ov: &NetworkOverride) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO network_override
                (network_id, original_ip1, original_ip2, original_ip3, original_ip4, original_port)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                ov.network_id,
                ov.original_ip1,
                ov.original_ip2,
                ov.original_ip3,
                ov.original_ip4,
                ov.original_port,
            ],
        )?;
        Ok(())
    }

    /// List all outstanding overrides.
    pub fn list_overrides(&self) -> Result<Vec<NetworkOverride>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT network_id, original_ip1, original_ip2, original_ip3, original_ip4, original_port
             FROM network_override",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(NetworkOverride {
                network_id: row.get(0)?,
                original_ip1: row.get(1)?,
                original_ip2: row.get(2)?,
                original_ip3: row.get(3)?,
                original_ip4: row.get(4)?,
                original_port: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Get the saved override for a network, if one exists.
    pub fn get_override(
        &self,
        network_id: i64,
    ) -> Result<Option<NetworkOverride>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT network_id, original_ip1, original_ip2, original_ip3, original_ip4, original_port
             FROM network_override WHERE network_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![network_id], |row| {
            Ok(NetworkOverride {
                network_id: row.get(0)?,
                original_ip1: row.get(1)?,
                original_ip2: row.get(2)?,
                original_ip3: row.get(3)?,
                original_ip4: row.get(4)?,
                original_port: row.get(5)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Remove the override record after reverting.
    pub fn clear_override(&self, network_id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM network_override WHERE network_id = ?1",
            params![network_id],
        )?;
        Ok(())
    }
}
