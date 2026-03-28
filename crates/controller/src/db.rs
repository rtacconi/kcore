use std::sync::{Arc, Mutex};

use anyhow::Result;
use rusqlite::{params, Connection};

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone)]
pub struct NodeRow {
    pub id: String,
    pub hostname: String,
    pub address: String,
    pub cpu_cores: i32,
    pub memory_bytes: i64,
    pub status: String,
    pub last_heartbeat: String,
    pub gateway_interface: String,
    pub cpu_used: i32,
    pub memory_used: i64,
    pub storage_backend: String,
    pub disable_vxlan: bool,
    pub approval_status: String,
    pub cert_expiry_days: i32,
    pub luks_method: String,
}

#[derive(Debug, Clone)]
pub struct VmRow {
    pub id: String,
    pub name: String,
    pub cpu: i32,
    pub memory_bytes: i64,
    pub image_path: String,
    pub image_url: String,
    pub image_sha256: String,
    pub image_format: String,
    pub image_size: i64,
    pub network: String,
    pub auto_start: bool,
    pub node_id: String,
    #[allow(dead_code)]
    pub created_at: String,
    pub runtime_state: String,
    pub cloud_init_user_data: String,
    pub storage_backend: String,
    pub storage_size_bytes: i64,
    pub vm_ip: String,
}

#[derive(Debug, Clone)]
pub struct NetworkRow {
    pub name: String,
    pub external_ip: String,
    pub gateway_ip: String,
    pub internal_netmask: String,
    pub node_id: String,
    pub allowed_tcp_ports: String,
    pub allowed_udp_ports: String,
    pub vlan_id: i32,
    pub network_type: String,
    pub enable_outbound_nat: bool,
    pub vni: i32,
    pub next_ip: i32,
}

#[derive(Debug, Clone)]
pub struct ReplicationOutboxRow {
    pub id: i64,
    #[allow(dead_code)]
    pub created_at: String,
    pub event_type: String,
    pub resource_key: String,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ReplicationAckRow {
    pub peer_id: String,
    pub last_event_id: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ReplicationResourceHeadRow {
    pub resource_key: String,
    pub last_op_id: String,
    pub last_logical_ts_unix_ms: i64,
    pub last_policy_priority: i32,
    pub last_intent_epoch: i64,
    pub last_validity: String,
    pub last_safety_class: String,
    pub last_controller_id: String,
    pub last_event_id: i64,
    pub last_event_type: String,
    pub last_body_json: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ReplicationConflictRow {
    pub id: i64,
    pub resource_key: String,
    pub incumbent_op_id: String,
    pub challenger_op_id: String,
    pub incumbent_controller_id: String,
    pub challenger_controller_id: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct ReplicationCompensationJobRow {
    pub id: i64,
    pub conflict_id: i64,
    pub resource_key: String,
    pub loser_op_id: String,
    pub status: String,
    pub attempts: i32,
}

#[derive(Debug, Clone)]
pub struct ReplicationMaterializedHeadRow {
    pub resource_key: String,
    pub last_op_id: String,
    pub last_event_type: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ReplicationReservationRow {
    pub reservation_key: String,
    pub resource_key: String,
    pub op_id: String,
    pub status: String,
    pub error: String,
    pub retry_count: i32,
}

impl Database {
    pub fn open(path: &str) -> Result<Self> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, rusqlite::Error> {
        self.conn.lock().map_err(|_| {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(1),
                Some("database mutex poisoned".to_string()),
            )
        })
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS nodes (
                id TEXT PRIMARY KEY,
                hostname TEXT NOT NULL,
                address TEXT NOT NULL,
                cpu_cores INTEGER NOT NULL DEFAULT 0,
                memory_bytes INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'unknown',
                last_heartbeat TEXT NOT NULL DEFAULT '',
                gateway_interface TEXT NOT NULL DEFAULT '',
                cpu_used INTEGER NOT NULL DEFAULT 0,
                memory_used INTEGER NOT NULL DEFAULT 0,
                storage_backend TEXT NOT NULL DEFAULT 'filesystem',
                disable_vxlan INTEGER NOT NULL DEFAULT 0,
                approval_status TEXT NOT NULL DEFAULT 'approved',
                cert_expiry_days INTEGER NOT NULL DEFAULT -1,
                luks_method TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS vms (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                cpu INTEGER NOT NULL,
                memory_bytes INTEGER NOT NULL,
                image_path TEXT NOT NULL,
                image_url TEXT NOT NULL DEFAULT '',
                image_sha256 TEXT NOT NULL DEFAULT '',
                image_format TEXT NOT NULL DEFAULT 'raw',
                image_size INTEGER NOT NULL DEFAULT 8192,
                network TEXT NOT NULL DEFAULT 'default',
                auto_start INTEGER NOT NULL DEFAULT 1,
                node_id TEXT NOT NULL REFERENCES nodes(id),
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                runtime_state TEXT NOT NULL DEFAULT 'unknown',
                cloud_init_user_data TEXT NOT NULL DEFAULT '',
                storage_backend TEXT NOT NULL DEFAULT 'filesystem',
                storage_size_bytes INTEGER NOT NULL DEFAULT 0,
                vm_ip TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS networks (
                name TEXT NOT NULL,
                external_ip TEXT NOT NULL,
                gateway_ip TEXT NOT NULL,
                internal_netmask TEXT NOT NULL DEFAULT '255.255.255.0',
                node_id TEXT NOT NULL REFERENCES nodes(id),
                allowed_tcp_ports TEXT NOT NULL DEFAULT '',
                allowed_udp_ports TEXT NOT NULL DEFAULT '',
                vlan_id INTEGER NOT NULL DEFAULT 0,
                network_type TEXT NOT NULL DEFAULT 'nat',
                enable_outbound_nat INTEGER NOT NULL DEFAULT 1,
                vni INTEGER NOT NULL DEFAULT 0,
                next_ip INTEGER NOT NULL DEFAULT 2,
                PRIMARY KEY (name, node_id)
            );
            CREATE TABLE IF NOT EXISTS node_labels (
                node_id TEXT NOT NULL REFERENCES nodes(id),
                label TEXT NOT NULL,
                PRIMARY KEY (node_id, label)
            );
            CREATE TABLE IF NOT EXISTS ssh_keys (
                name TEXT PRIMARY KEY,
                public_key TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS vm_ssh_keys (
                vm_id TEXT NOT NULL REFERENCES vms(id) ON DELETE CASCADE,
                key_name TEXT NOT NULL REFERENCES ssh_keys(name),
                PRIMARY KEY (vm_id, key_name)
            );",
        )?;

        let version = Self::schema_version(&conn);

        if version < 1 {
            let _ = conn.execute(
                "ALTER TABLE vms ADD COLUMN image_url TEXT NOT NULL DEFAULT ''",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE vms ADD COLUMN image_sha256 TEXT NOT NULL DEFAULT ''",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE vms ADD COLUMN image_format TEXT NOT NULL DEFAULT 'raw'",
                [],
            );
            let _ = conn.execute(
                "UPDATE vms
                 SET image_format = CASE
                     WHEN lower(image_path) LIKE '%.qcow2' OR lower(image_path) LIKE '%.qcow' THEN 'qcow2'
                     ELSE 'raw'
                 END
                 WHERE image_format IS NULL
                    OR image_format = ''
                    OR (image_format = 'raw' AND image_url != '' AND (lower(image_path) LIKE '%.qcow2' OR lower(image_path) LIKE '%.qcow'))",
                [],
            );
        }

        if version < 2 {
            let _ = conn.execute(
                "ALTER TABLE vms ADD COLUMN runtime_state TEXT NOT NULL DEFAULT 'unknown'",
                [],
            );
        }

        if version < 3 {
            let _ = conn.execute(
                "ALTER TABLE nodes ADD COLUMN cpu_used INTEGER NOT NULL DEFAULT 0",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE nodes ADD COLUMN memory_used INTEGER NOT NULL DEFAULT 0",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE networks ADD COLUMN allowed_tcp_ports TEXT NOT NULL DEFAULT ''",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE networks ADD COLUMN allowed_udp_ports TEXT NOT NULL DEFAULT ''",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE vms ADD COLUMN cloud_init_user_data TEXT NOT NULL DEFAULT ''",
                [],
            );
        }

        if version < 4 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS ssh_keys (
                    name TEXT PRIMARY KEY,
                    public_key TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );
                CREATE TABLE IF NOT EXISTS vm_ssh_keys (
                    vm_id TEXT NOT NULL REFERENCES vms(id) ON DELETE CASCADE,
                    key_name TEXT NOT NULL REFERENCES ssh_keys(name),
                    PRIMARY KEY (vm_id, key_name)
                );",
            );
        }

        if version < 5 {
            let _ = conn.execute(
                "ALTER TABLE nodes ADD COLUMN storage_backend TEXT NOT NULL DEFAULT 'filesystem'",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE vms ADD COLUMN storage_backend TEXT NOT NULL DEFAULT 'filesystem'",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE vms ADD COLUMN storage_size_bytes INTEGER NOT NULL DEFAULT 0",
                [],
            );
        }

        if version < 6 {
            let _ = conn.execute(
                "ALTER TABLE networks ADD COLUMN vlan_id INTEGER NOT NULL DEFAULT 0",
                [],
            );
        }

        if version < 7 {
            let _ = conn.execute(
                "ALTER TABLE networks ADD COLUMN network_type TEXT NOT NULL DEFAULT 'nat'",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE networks ADD COLUMN enable_outbound_nat INTEGER NOT NULL DEFAULT 1",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE networks ADD COLUMN vni INTEGER NOT NULL DEFAULT 0",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE networks ADD COLUMN next_ip INTEGER NOT NULL DEFAULT 2",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE vms ADD COLUMN vm_ip TEXT NOT NULL DEFAULT ''",
                [],
            );
        }

        if version < 8 {
            let _ = conn.execute(
                "ALTER TABLE nodes ADD COLUMN disable_vxlan INTEGER NOT NULL DEFAULT 0",
                [],
            );
        }

        if version < 9 {
            let _ = conn.execute(
                "ALTER TABLE nodes ADD COLUMN approval_status TEXT NOT NULL DEFAULT 'approved'",
                [],
            );
        }

        if version < 10 {
            let _ = conn.execute(
                "ALTER TABLE nodes ADD COLUMN cert_expiry_days INTEGER NOT NULL DEFAULT -1",
                [],
            );
        }

        if version < 11 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS replication_outbox (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    event_type TEXT NOT NULL,
                    resource_key TEXT NOT NULL,
                    payload BLOB NOT NULL
                );",
            );
        }

        if version < 12 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS replication_ack (
                    peer_id TEXT PRIMARY KEY,
                    last_event_id INTEGER NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
                );",
            );
        }

        if version < 13 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS replication_received_ops (
                    op_id TEXT PRIMARY KEY,
                    origin_controller_id TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    resource_key TEXT NOT NULL,
                    first_seen_at TEXT NOT NULL DEFAULT (datetime('now'))
                );",
            );
        }

        if version < 14 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS replication_resource_heads (
                    resource_key TEXT PRIMARY KEY,
                    last_op_id TEXT NOT NULL,
                    last_logical_ts_unix_ms INTEGER NOT NULL,
                    last_controller_id TEXT NOT NULL,
                    last_event_id INTEGER NOT NULL,
                    last_event_type TEXT NOT NULL,
                    last_body_json TEXT NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
                );",
            );
        }

        if version < 15 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS replication_conflicts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    resource_key TEXT NOT NULL,
                    incumbent_op_id TEXT NOT NULL,
                    challenger_op_id TEXT NOT NULL,
                    incumbent_controller_id TEXT NOT NULL,
                    challenger_controller_id TEXT NOT NULL,
                    reason TEXT NOT NULL,
                    resolved INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );",
            );
        }

        if version < 16 {
            let _ = conn.execute(
                "ALTER TABLE replication_resource_heads ADD COLUMN last_policy_priority INTEGER NOT NULL DEFAULT 0",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE replication_resource_heads ADD COLUMN last_intent_epoch INTEGER NOT NULL DEFAULT 0",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE replication_resource_heads ADD COLUMN last_validity TEXT NOT NULL DEFAULT 'valid'",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE replication_resource_heads ADD COLUMN last_safety_class TEXT NOT NULL DEFAULT 'safe'",
                [],
            );
        }

        if version < 17 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS replication_compensation_jobs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    conflict_id INTEGER NOT NULL REFERENCES replication_conflicts(id) ON DELETE CASCADE,
                    resource_key TEXT NOT NULL,
                    loser_op_id TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    attempts INTEGER NOT NULL DEFAULT 0,
                    last_error TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
                );",
            );
        }

        if version < 18 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS replication_materialized_heads (
                    resource_key TEXT PRIMARY KEY,
                    last_op_id TEXT NOT NULL,
                    last_event_type TEXT NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
                );",
            );
        }

        if version < 19 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS replication_reservations (
                    reservation_key TEXT NOT NULL,
                    resource_key TEXT NOT NULL,
                    op_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    error TEXT NOT NULL DEFAULT '',
                    retry_count INTEGER NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                    PRIMARY KEY (reservation_key, resource_key)
                );",
            );
        }

        if version < 20 {
            let _ = conn.execute(
                "ALTER TABLE replication_reservations ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0",
                [],
            );
        }

        if version < 21 {
            let _ = conn.execute(
                "ALTER TABLE nodes ADD COLUMN luks_method TEXT NOT NULL DEFAULT ''",
                [],
            );
        }

        const CURRENT_VERSION: i32 = 21;
        if version < CURRENT_VERSION {
            conn.execute("DELETE FROM schema_version", [])?;
            conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                params![CURRENT_VERSION],
            )?;
        }

        Ok(())
    }

    fn schema_version(conn: &Connection) -> i32 {
        conn.query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap_or(0)
    }

    pub fn append_replication_outbox(
        &self,
        event_type: &str,
        resource_key: &str,
        payload: &[u8],
    ) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO replication_outbox (event_type, resource_key, payload) VALUES (?1, ?2, ?3)",
            params![event_type, resource_key, payload],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn replication_outbox_len(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.query_row("SELECT COUNT(*) FROM replication_outbox", [], |row| {
            row.get(0)
        })
    }

    pub fn replication_outbox_head_id(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT COALESCE(MAX(id), 0) FROM replication_outbox",
            [],
            |row| row.get(0),
        )
    }

    pub fn list_replication_outbox_since(
        &self,
        min_id_exclusive: i64,
        limit: i64,
    ) -> Result<Vec<ReplicationOutboxRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, created_at, event_type, resource_key, payload
             FROM replication_outbox
             WHERE id > ?1
             ORDER BY id ASC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![min_id_exclusive, limit], |row| {
            Ok(ReplicationOutboxRow {
                id: row.get(0)?,
                created_at: row.get(1)?,
                event_type: row.get(2)?,
                resource_key: row.get(3)?,
                payload: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn upsert_replication_ack(
        &self,
        peer_id: &str,
        last_event_id: i64,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO replication_ack (peer_id, last_event_id, updated_at)
             VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(peer_id) DO UPDATE SET
                last_event_id=excluded.last_event_id,
                updated_at=datetime('now')",
            params![peer_id, last_event_id],
        )?;
        Ok(())
    }

    pub fn get_replication_ack(&self, peer_id: &str) -> Result<Option<i64>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        match conn.query_row(
            "SELECT last_event_id FROM replication_ack WHERE peer_id = ?1",
            params![peer_id],
            |row| row.get(0),
        ) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn list_replication_acks(&self) -> Result<Vec<ReplicationAckRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT peer_id, last_event_id, updated_at
             FROM replication_ack
             ORDER BY peer_id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ReplicationAckRow {
                peer_id: row.get(0)?,
                last_event_id: row.get(1)?,
                updated_at: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn replication_received_op_exists(&self, op_id: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM replication_received_ops WHERE op_id = ?1",
            params![op_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn insert_replication_received_op(
        &self,
        op_id: &str,
        origin_controller_id: &str,
        event_type: &str,
        resource_key: &str,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO replication_received_ops (op_id, origin_controller_id, event_type, resource_key)
             VALUES (?1, ?2, ?3, ?4)",
            params![op_id, origin_controller_id, event_type, resource_key],
        )?;
        Ok(())
    }

    pub fn get_replication_resource_head(
        &self,
        resource_key: &str,
    ) -> Result<Option<ReplicationResourceHeadRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT resource_key, last_op_id, last_logical_ts_unix_ms,
                    last_policy_priority, last_intent_epoch, last_validity, last_safety_class,
                    last_controller_id, last_event_id, last_event_type, last_body_json
             FROM replication_resource_heads
             WHERE resource_key = ?1",
        )?;
        let mut rows = stmt.query_map(params![resource_key], |row| {
            Ok(ReplicationResourceHeadRow {
                resource_key: row.get(0)?,
                last_op_id: row.get(1)?,
                last_logical_ts_unix_ms: row.get(2)?,
                last_policy_priority: row.get(3)?,
                last_intent_epoch: row.get(4)?,
                last_validity: row.get(5)?,
                last_safety_class: row.get(6)?,
                last_controller_id: row.get(7)?,
                last_event_id: row.get(8)?,
                last_event_type: row.get(9)?,
                last_body_json: row.get(10)?,
            })
        })?;
        rows.next().transpose()
    }

    pub fn upsert_replication_resource_head(
        &self,
        row: &ReplicationResourceHeadRow,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO replication_resource_heads (
                resource_key, last_op_id, last_logical_ts_unix_ms,
                last_policy_priority, last_intent_epoch, last_validity, last_safety_class,
                last_controller_id,
                last_event_id, last_event_type, last_body_json, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, datetime('now'))
             ON CONFLICT(resource_key) DO UPDATE SET
                last_op_id=excluded.last_op_id,
                last_logical_ts_unix_ms=excluded.last_logical_ts_unix_ms,
                last_policy_priority=excluded.last_policy_priority,
                last_intent_epoch=excluded.last_intent_epoch,
                last_validity=excluded.last_validity,
                last_safety_class=excluded.last_safety_class,
                last_controller_id=excluded.last_controller_id,
                last_event_id=excluded.last_event_id,
                last_event_type=excluded.last_event_type,
                last_body_json=excluded.last_body_json,
                updated_at=datetime('now')",
            params![
                &row.resource_key,
                &row.last_op_id,
                row.last_logical_ts_unix_ms,
                row.last_policy_priority,
                row.last_intent_epoch,
                &row.last_validity,
                &row.last_safety_class,
                &row.last_controller_id,
                row.last_event_id,
                &row.last_event_type,
                &row.last_body_json,
            ],
        )?;
        Ok(())
    }

    pub fn insert_replication_conflict(
        &self,
        resource_key: &str,
        incumbent_op_id: &str,
        challenger_op_id: &str,
        incumbent_controller_id: &str,
        challenger_controller_id: &str,
        reason: &str,
    ) -> Result<i64, rusqlite::Error> {
        self.insert_replication_conflict_with_resolved(
            resource_key,
            incumbent_op_id,
            challenger_op_id,
            incumbent_controller_id,
            challenger_controller_id,
            reason,
            false,
        )
    }

    pub fn insert_replication_conflict_with_resolved(
        &self,
        resource_key: &str,
        incumbent_op_id: &str,
        challenger_op_id: &str,
        incumbent_controller_id: &str,
        challenger_controller_id: &str,
        reason: &str,
        resolved: bool,
    ) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO replication_conflicts (
                resource_key, incumbent_op_id, challenger_op_id,
                incumbent_controller_id, challenger_controller_id, reason, resolved
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                resource_key,
                incumbent_op_id,
                challenger_op_id,
                incumbent_controller_id,
                challenger_controller_id,
                reason,
                if resolved { 1 } else { 0 }
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn count_unresolved_replication_conflicts(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT COUNT(*) FROM replication_conflicts WHERE resolved = 0",
            [],
            |row| row.get(0),
        )
    }

    pub fn list_unresolved_replication_conflicts(
        &self,
        limit: i64,
    ) -> Result<Vec<ReplicationConflictRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, resource_key, incumbent_op_id, challenger_op_id,
                    incumbent_controller_id, challenger_controller_id, reason
             FROM replication_conflicts
             WHERE resolved = 0
             ORDER BY id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(ReplicationConflictRow {
                id: row.get(0)?,
                resource_key: row.get(1)?,
                incumbent_op_id: row.get(2)?,
                challenger_op_id: row.get(3)?,
                incumbent_controller_id: row.get(4)?,
                challenger_controller_id: row.get(5)?,
                reason: row.get(6)?,
            })
        })?;
        rows.collect()
    }

    pub fn resolve_replication_conflict(&self, id: i64) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "UPDATE replication_conflicts
             SET resolved = 1
             WHERE id = ?1 AND resolved = 0",
            params![id],
        )?;
        Ok(rows > 0)
    }

    pub fn insert_compensation_job(
        &self,
        conflict_id: i64,
        resource_key: &str,
        loser_op_id: &str,
    ) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO replication_compensation_jobs (conflict_id, resource_key, loser_op_id)
             VALUES (?1, ?2, ?3)",
            params![conflict_id, resource_key, loser_op_id],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn count_pending_compensation_jobs(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT COUNT(*) FROM replication_compensation_jobs WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )
    }

    pub fn count_failed_compensation_jobs(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT COUNT(*) FROM replication_compensation_jobs WHERE status = 'failed'",
            [],
            |row| row.get(0),
        )
    }

    pub fn claim_next_compensation_job(
        &self,
    ) -> Result<Option<ReplicationCompensationJobRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, conflict_id, resource_key, loser_op_id, status, attempts
             FROM replication_compensation_jobs
             WHERE status IN ('pending', 'failed')
             ORDER BY id ASC
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map([], |row| {
            Ok(ReplicationCompensationJobRow {
                id: row.get(0)?,
                conflict_id: row.get(1)?,
                resource_key: row.get(2)?,
                loser_op_id: row.get(3)?,
                status: row.get(4)?,
                attempts: row.get(5)?,
            })
        })?;
        let Some(job) = rows.next().transpose()? else {
            return Ok(None);
        };
        let updated = conn.execute(
            "UPDATE replication_compensation_jobs
             SET status = 'running', attempts = attempts + 1, updated_at = datetime('now')
             WHERE id = ?1 AND status IN ('pending', 'failed')",
            params![job.id],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        let mut claimed = job;
        claimed.status = "running".to_string();
        claimed.attempts += 1;
        Ok(Some(claimed))
    }

    pub fn complete_compensation_job(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE replication_compensation_jobs
             SET status = 'completed', updated_at = datetime('now')
             WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn fail_compensation_job(&self, id: i64, error: &str) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE replication_compensation_jobs
             SET status = 'failed', last_error = ?2, updated_at = datetime('now')
             WHERE id = ?1",
            params![id, error],
        )?;
        Ok(())
    }

    pub fn get_compensation_job_status_for_loser_op(
        &self,
        loser_op_id: &str,
    ) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT status
             FROM replication_compensation_jobs
             WHERE loser_op_id = ?1
             ORDER BY id DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![loser_op_id], |row| row.get::<_, String>(0))?;
        rows.next().transpose()
    }

    pub fn list_replication_resource_heads(
        &self,
        limit: i64,
    ) -> Result<Vec<ReplicationResourceHeadRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT resource_key, last_op_id, last_logical_ts_unix_ms,
                    last_policy_priority, last_intent_epoch, last_validity, last_safety_class,
                    last_controller_id, last_event_id, last_event_type, last_body_json
             FROM replication_resource_heads
             ORDER BY last_event_id ASC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(ReplicationResourceHeadRow {
                resource_key: row.get(0)?,
                last_op_id: row.get(1)?,
                last_logical_ts_unix_ms: row.get(2)?,
                last_policy_priority: row.get(3)?,
                last_intent_epoch: row.get(4)?,
                last_validity: row.get(5)?,
                last_safety_class: row.get(6)?,
                last_controller_id: row.get(7)?,
                last_event_id: row.get(8)?,
                last_event_type: row.get(9)?,
                last_body_json: row.get(10)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_materialized_replication_head(
        &self,
        resource_key: &str,
    ) -> Result<Option<ReplicationMaterializedHeadRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT resource_key, last_op_id, last_event_type
             FROM replication_materialized_heads
             WHERE resource_key = ?1",
        )?;
        let mut rows = stmt.query_map(params![resource_key], |row| {
            Ok(ReplicationMaterializedHeadRow {
                resource_key: row.get(0)?,
                last_op_id: row.get(1)?,
                last_event_type: row.get(2)?,
            })
        })?;
        rows.next().transpose()
    }

    pub fn upsert_materialized_replication_head(
        &self,
        resource_key: &str,
        last_op_id: &str,
        last_event_type: &str,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO replication_materialized_heads (resource_key, last_op_id, last_event_type, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(resource_key) DO UPDATE SET
                last_op_id=excluded.last_op_id,
                last_event_type=excluded.last_event_type,
                updated_at=datetime('now')",
            params![resource_key, last_op_id, last_event_type],
        )?;
        Ok(())
    }

    pub fn upsert_replication_reservation(
        &self,
        reservation_key: &str,
        resource_key: &str,
        op_id: &str,
        status: &str,
        error: &str,
    ) -> Result<(), rusqlite::Error> {
        self.upsert_replication_reservation_with_retry(
            reservation_key,
            resource_key,
            op_id,
            status,
            error,
            0,
        )
    }

    pub fn upsert_replication_reservation_with_retry(
        &self,
        reservation_key: &str,
        resource_key: &str,
        op_id: &str,
        status: &str,
        error: &str,
        retry_count: i32,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO replication_reservations (
                reservation_key, resource_key, op_id, status, error, retry_count, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
             ON CONFLICT(reservation_key, resource_key) DO UPDATE SET
                op_id=excluded.op_id,
                status=excluded.status,
                error=excluded.error,
                retry_count=excluded.retry_count,
                updated_at=datetime('now')",
            params![
                reservation_key,
                resource_key,
                op_id,
                status,
                error,
                retry_count
            ],
        )?;
        Ok(())
    }

    pub fn record_replication_reservation_failure(
        &self,
        reservation_key: &str,
        resource_key: &str,
        op_id: &str,
        retryable: bool,
        error: &str,
        max_retries: i32,
    ) -> Result<(String, i32), rusqlite::Error> {
        let current_retry = self
            .get_replication_reservation(reservation_key, resource_key)?
            .map(|r| r.retry_count)
            .unwrap_or(0);
        let next_retry = current_retry + 1;
        let status = if retryable {
            if next_retry >= max_retries {
                "retry_exhausted"
            } else {
                "failed_retryable"
            }
        } else {
            "failed_non_retryable"
        };
        self.upsert_replication_reservation_with_retry(
            reservation_key,
            resource_key,
            op_id,
            status,
            error,
            next_retry,
        )?;
        Ok((status.to_string(), next_retry))
    }

    pub fn count_failed_replication_reservations(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT COUNT(*) FROM replication_reservations
             WHERE status IN ('failed_retryable', 'failed_non_retryable', 'retry_exhausted')",
            [],
            |row| row.get(0),
        )
    }

    pub fn count_failed_retryable_replication_reservations(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT COUNT(*) FROM replication_reservations WHERE status = 'failed_retryable'",
            [],
            |row| row.get(0),
        )
    }

    pub fn count_failed_non_retryable_replication_reservations(
        &self,
    ) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT COUNT(*) FROM replication_reservations WHERE status = 'failed_non_retryable'",
            [],
            |row| row.get(0),
        )
    }

    pub fn count_retry_exhausted_replication_reservations(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT COUNT(*) FROM replication_reservations WHERE status = 'retry_exhausted'",
            [],
            |row| row.get(0),
        )
    }

    pub fn list_retryable_replication_reservations(
        &self,
        limit: i64,
        min_age_seconds: i64,
    ) -> Result<Vec<ReplicationReservationRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT reservation_key, resource_key, op_id, status, error, retry_count
             FROM replication_reservations
             WHERE status = 'failed_retryable'
               AND ((julianday('now') - julianday(updated_at)) * 86400) >= ?1
             ORDER BY updated_at ASC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![min_age_seconds, limit], |row| {
            Ok(ReplicationReservationRow {
                reservation_key: row.get(0)?,
                resource_key: row.get(1)?,
                op_id: row.get(2)?,
                status: row.get(3)?,
                error: row.get(4)?,
                retry_count: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_replication_reservation(
        &self,
        reservation_key: &str,
        resource_key: &str,
    ) -> Result<Option<ReplicationReservationRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT reservation_key, resource_key, op_id, status, error, retry_count
             FROM replication_reservations
             WHERE reservation_key = ?1 AND resource_key = ?2",
        )?;
        let mut rows = stmt.query_map(params![reservation_key, resource_key], |row| {
            Ok(ReplicationReservationRow {
                reservation_key: row.get(0)?,
                resource_key: row.get(1)?,
                op_id: row.get(2)?,
                status: row.get(3)?,
                error: row.get(4)?,
                retry_count: row.get(5)?,
            })
        })?;
        rows.next().transpose()
    }

    pub fn count_replication_materialization_backlog(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT COUNT(*)
             FROM replication_resource_heads h
             LEFT JOIN replication_materialized_heads m
               ON m.resource_key = h.resource_key
             WHERE m.last_op_id IS NULL OR m.last_op_id != h.last_op_id",
            [],
            |row| row.get(0),
        )
    }

    pub fn oldest_unresolved_conflict_age_seconds(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT COALESCE(CAST(MAX((julianday('now') - julianday(created_at)) * 86400) AS INTEGER), 0)
             FROM replication_conflicts
             WHERE resolved = 0",
            [],
            |row| row.get(0),
        )
    }

    pub fn upsert_node(&self, node: &NodeRow) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO nodes (id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface, cpu_used, memory_used, storage_backend, disable_vxlan, approval_status, cert_expiry_days, luks_method)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(id) DO UPDATE SET
                hostname=excluded.hostname,
                address=excluded.address,
                cpu_cores=excluded.cpu_cores,
                memory_bytes=excluded.memory_bytes,
                status=excluded.status,
                last_heartbeat=excluded.last_heartbeat,
                gateway_interface=excluded.gateway_interface,
                cpu_used=excluded.cpu_used,
                memory_used=excluded.memory_used,
                storage_backend=excluded.storage_backend,
                disable_vxlan=excluded.disable_vxlan,
                cert_expiry_days=excluded.cert_expiry_days,
                luks_method=excluded.luks_method",
            params![
                node.id,
                node.hostname,
                node.address,
                node.cpu_cores,
                node.memory_bytes,
                node.status,
                node.last_heartbeat,
                node.gateway_interface,
                node.cpu_used,
                node.memory_used,
                node.storage_backend,
                node.disable_vxlan as i32,
                node.approval_status,
                node.cert_expiry_days,
                node.luks_method,
            ],
        )?;
        Ok(())
    }

    pub fn set_node_approval(&self, node_id: &str, status: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "UPDATE nodes SET approval_status = ?2 WHERE id = ?1",
            params![node_id, status],
        )?;
        Ok(rows > 0)
    }

    pub fn update_heartbeat(
        &self,
        node_id: &str,
        cpu_used: i32,
        mem_used: i64,
        cert_expiry_days: i32,
        luks_method: &str,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "UPDATE nodes SET last_heartbeat = datetime('now'), status = 'ready', cpu_used = ?2, memory_used = ?3, cert_expiry_days = ?4, luks_method = ?5 WHERE id = ?1 AND approval_status = 'approved'",
            params![node_id, cpu_used, mem_used, cert_expiry_days, luks_method],
        )?;
        Ok(rows > 0)
    }

    pub fn get_node(&self, node_id: &str) -> Result<Option<NodeRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface, cpu_used, memory_used, storage_backend, disable_vxlan, approval_status, cert_expiry_days, luks_method FROM nodes WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![node_id], row_to_node)?;
        rows.next().transpose()
    }

    pub fn list_nodes(&self) -> Result<Vec<NodeRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface, cpu_used, memory_used, storage_backend, disable_vxlan, approval_status, cert_expiry_days, luks_method FROM nodes",
        )?;
        let rows = stmt.query_map([], row_to_node)?;
        rows.collect()
    }

    pub fn get_node_by_address(&self, address: &str) -> Result<Option<NodeRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface, cpu_used, memory_used, storage_backend, disable_vxlan, approval_status, cert_expiry_days, luks_method FROM nodes WHERE address = ?1",
        )?;
        let mut rows = stmt.query_map(params![address], row_to_node)?;
        rows.next().transpose()
    }

    pub fn insert_vm(&self, vm: &VmRow) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO vms (id, name, cpu, memory_bytes, image_path, image_url, image_sha256, image_format, image_size, network, auto_start, node_id, created_at, runtime_state, cloud_init_user_data, storage_backend, storage_size_bytes, vm_ip)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, datetime('now'), ?13, ?14, ?15, ?16, ?17)",
            params![
                vm.id,
                vm.name,
                vm.cpu,
                vm.memory_bytes,
                vm.image_path,
                vm.image_url,
                vm.image_sha256,
                vm.image_format,
                vm.image_size,
                vm.network,
                vm.auto_start as i32,
                vm.node_id,
                vm.runtime_state,
                vm.cloud_init_user_data,
                vm.storage_backend,
                vm.storage_size_bytes,
                vm.vm_ip,
            ],
        )?;
        Ok(())
    }

    pub fn get_vm(&self, vm_id: &str) -> Result<Option<VmRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, cpu, memory_bytes, image_path, image_url, image_sha256, image_format, image_size, network, auto_start, node_id, created_at, runtime_state, cloud_init_user_data, storage_backend, storage_size_bytes, vm_ip FROM vms WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![vm_id], row_to_vm)?;
        rows.next().transpose()
    }

    pub fn list_vms(&self) -> Result<Vec<VmRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, cpu, memory_bytes, image_path, image_url, image_sha256, image_format, image_size, network, auto_start, node_id, created_at, runtime_state, cloud_init_user_data, storage_backend, storage_size_bytes, vm_ip FROM vms",
        )?;
        let rows = stmt.query_map([], row_to_vm)?;
        rows.collect()
    }

    pub fn list_vms_for_node(&self, node_id: &str) -> Result<Vec<VmRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, cpu, memory_bytes, image_path, image_url, image_sha256, image_format, image_size, network, auto_start, node_id, created_at, runtime_state, cloud_init_user_data, storage_backend, storage_size_bytes, vm_ip FROM vms WHERE node_id = ?1",
        )?;
        let rows = stmt.query_map(params![node_id], row_to_vm)?;
        rows.collect()
    }

    pub fn insert_network(&self, network: &NetworkRow) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO networks (name, external_ip, gateway_ip, internal_netmask, node_id, allowed_tcp_ports, allowed_udp_ports, vlan_id, network_type, enable_outbound_nat, vni, next_ip)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                network.name,
                network.external_ip,
                network.gateway_ip,
                network.internal_netmask,
                network.node_id,
                network.allowed_tcp_ports,
                network.allowed_udp_ports,
                network.vlan_id,
                network.network_type,
                network.enable_outbound_nat as i32,
                network.vni,
                network.next_ip,
            ],
        )?;
        Ok(())
    }

    pub fn get_network_for_node(
        &self,
        node_id: &str,
        name: &str,
    ) -> Result<Option<NetworkRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT name, external_ip, gateway_ip, internal_netmask, node_id, allowed_tcp_ports, allowed_udp_ports, vlan_id, network_type, enable_outbound_nat, vni, next_ip
             FROM networks
             WHERE node_id = ?1 AND name = ?2",
        )?;
        let mut rows = stmt.query_map(params![node_id, name], row_to_network)?;
        rows.next().transpose()
    }

    pub fn list_networks(&self) -> Result<Vec<NetworkRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT name, external_ip, gateway_ip, internal_netmask, node_id, allowed_tcp_ports, allowed_udp_ports, vlan_id, network_type, enable_outbound_nat, vni, next_ip
             FROM networks",
        )?;
        let rows = stmt.query_map([], row_to_network)?;
        rows.collect()
    }

    pub fn list_networks_for_node(
        &self,
        node_id: &str,
    ) -> Result<Vec<NetworkRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT name, external_ip, gateway_ip, internal_netmask, node_id, allowed_tcp_ports, allowed_udp_ports, vlan_id, network_type, enable_outbound_nat, vni, next_ip
             FROM networks
             WHERE node_id = ?1",
        )?;
        let rows = stmt.query_map(params![node_id], row_to_network)?;
        rows.collect()
    }

    pub fn list_networks_by_name(&self, name: &str) -> Result<Vec<NetworkRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT name, external_ip, gateway_ip, internal_netmask, node_id, allowed_tcp_ports, allowed_udp_ports, vlan_id, network_type, enable_outbound_nat, vni, next_ip
             FROM networks
             WHERE name = ?1",
        )?;
        let rows = stmt.query_map(params![name], row_to_network)?;
        rows.collect()
    }

    pub fn allocate_vm_ip(
        &self,
        network_name: &str,
        node_id: &str,
    ) -> Result<String, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let (gateway_ip, next_ip): (String, i32) = conn.query_row(
            "SELECT gateway_ip, next_ip FROM networks WHERE name = ?1 AND node_id = ?2",
            params![network_name, node_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        conn.execute(
            "UPDATE networks SET next_ip = next_ip + 1 WHERE name = ?1 AND node_id = ?2",
            params![network_name, node_id],
        )?;
        let prefix = gateway_ip.rsplit_once('.').map(|x| x.0).unwrap_or("10.0.0");
        Ok(format!("{}.{}", prefix, next_ip))
    }

    pub fn delete_network(&self, node_id: &str, name: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "DELETE FROM networks WHERE node_id = ?1 AND name = ?2",
            params![node_id, name],
        )?;
        Ok(rows > 0)
    }

    pub fn find_node_for_vm(&self, vm_id: &str) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt =
            conn.prepare("SELECT node_id FROM vms WHERE id = ?1 OR name = ?1 LIMIT 1")?;
        let mut rows = stmt.query_map(params![vm_id], |row| row.get::<_, String>(0))?;
        rows.next().transpose()
    }

    pub fn set_vm_auto_start(
        &self,
        vm_id_or_name: &str,
        auto_start: bool,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "UPDATE vms SET auto_start = ?1 WHERE id = ?2 OR name = ?2",
            params![auto_start as i32, vm_id_or_name],
        )?;
        Ok(rows > 0)
    }

    pub fn update_vm_runtime_state(
        &self,
        node_id: &str,
        vm_name: &str,
        state: &str,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "UPDATE vms SET runtime_state = ?1 WHERE name = ?2 AND node_id = ?3",
            params![state, vm_name, node_id],
        )?;
        Ok(rows > 0)
    }

    pub fn delete_vm_by_id_or_name(&self, id_or_name: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "DELETE FROM vms WHERE id = ?1 OR name = ?1",
            params![id_or_name],
        )?;
        Ok(rows > 0)
    }

    pub fn update_vm_spec(
        &self,
        id_or_name: &str,
        cpu: Option<i32>,
        memory_bytes: Option<i64>,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut parts = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(c) = cpu {
            parts.push("cpu = ?");
            values.push(Box::new(c));
        }
        if let Some(m) = memory_bytes {
            parts.push("memory_bytes = ?");
            values.push(Box::new(m));
        }
        if parts.is_empty() {
            return Ok(false);
        }
        values.push(Box::new(id_or_name.to_string()));
        let set_clause = parts
            .iter()
            .enumerate()
            .map(|(i, p)| p.replace('?', &format!("?{}", i + 1)))
            .collect::<Vec<_>>()
            .join(", ");
        let id_param = values.len();
        let sql = format!(
            "UPDATE vms SET {} WHERE id = ?{} OR name = ?{}",
            set_clause, id_param, id_param
        );
        let refs: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let rows = conn.execute(&sql, refs.as_slice())?;
        Ok(rows > 0)
    }

    pub fn upsert_node_labels(
        &self,
        node_id: &str,
        labels: &[String],
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "DELETE FROM node_labels WHERE node_id = ?1",
            params![node_id],
        )?;
        for label in labels {
            conn.execute(
                "INSERT INTO node_labels (node_id, label) VALUES (?1, ?2)",
                params![node_id, label],
            )?;
        }
        Ok(())
    }

    pub fn get_node_labels(&self, node_id: &str) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare("SELECT label FROM node_labels WHERE node_id = ?1")?;
        let rows = stmt.query_map(params![node_id], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    pub fn get_all_node_labels(&self) -> Result<Vec<(String, String)>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare("SELECT node_id, label FROM node_labels ORDER BY node_id")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect()
    }

    pub fn insert_ssh_key(&self, name: &str, public_key: &str) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO ssh_keys (name, public_key) VALUES (?1, ?2)",
            params![name, public_key],
        )?;
        Ok(())
    }

    pub fn get_ssh_key(
        &self,
        name: &str,
    ) -> Result<Option<(String, String, String)>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt =
            conn.prepare("SELECT name, public_key, created_at FROM ssh_keys WHERE name = ?1")?;
        let mut rows = stmt.query_map(params![name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.next().transpose()
    }

    pub fn list_ssh_keys(&self) -> Result<Vec<(String, String, String)>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt =
            conn.prepare("SELECT name, public_key, created_at FROM ssh_keys ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect()
    }

    pub fn delete_ssh_key(&self, name: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute("DELETE FROM ssh_keys WHERE name = ?1", params![name])?;
        Ok(rows > 0)
    }

    pub fn associate_vm_ssh_keys(
        &self,
        vm_id: &str,
        key_names: &[String],
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        for key_name in key_names {
            conn.execute(
                "INSERT OR IGNORE INTO vm_ssh_keys (vm_id, key_name) VALUES (?1, ?2)",
                params![vm_id, key_name],
            )?;
        }
        Ok(())
    }

    pub fn get_vm_ssh_keys(&self, vm_id: &str) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT sk.public_key FROM vm_ssh_keys vsk JOIN ssh_keys sk ON vsk.key_name = sk.name WHERE vsk.vm_id = ?1",
        )?;
        let rows = stmt.query_map(params![vm_id], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    pub fn update_node_status(&self, node_id: &str, status: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "UPDATE nodes SET status = ?1 WHERE id = ?2",
            params![status, node_id],
        )?;
        Ok(rows > 0)
    }

    pub fn get_stale_nodes(&self, timeout_seconds: i64) -> Result<Vec<NodeRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface, cpu_used, memory_used, storage_backend, disable_vxlan FROM nodes WHERE status = 'ready' AND last_heartbeat != '' AND (julianday('now') - julianday(last_heartbeat)) * 86400 > ?1",
        )?;
        let rows = stmt.query_map(params![timeout_seconds], row_to_node)?;
        rows.collect()
    }

    pub fn count_nodes_by_approval(&self) -> Result<(i32, i32, i32), rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt =
            conn.prepare("SELECT approval_status, COUNT(*) FROM nodes GROUP BY approval_status")?;
        let rows = stmt.query_map([], |row| {
            let status: String = row.get(0)?;
            let count: i32 = row.get(1)?;
            Ok((status, count))
        })?;
        let (mut approved, mut pending, mut rejected) = (0, 0, 0);
        for row in rows {
            let (status, count) = row?;
            match status.as_str() {
                "approved" => approved = count,
                "pending" => pending = count,
                "rejected" => rejected = count,
                _ => {}
            }
        }
        Ok((approved, pending, rejected))
    }

    pub fn count_vms_by_auto_start(&self) -> Result<(i32, i32), rusqlite::Error> {
        let conn = self.lock_conn()?;
        let total: i32 = conn.query_row("SELECT COUNT(*) FROM vms", [], |row| row.get(0))?;
        let running: i32 =
            conn.query_row("SELECT COUNT(*) FROM vms WHERE auto_start = 1", [], |row| {
                row.get(0)
            })?;
        Ok((total, running))
    }

    pub fn count_networks_by_type(&self) -> Result<(i32, i32, i32), rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt =
            conn.prepare("SELECT network_type, COUNT(*) FROM networks GROUP BY network_type")?;
        let rows = stmt.query_map([], |row| {
            let ntype: String = row.get(0)?;
            let count: i32 = row.get(1)?;
            Ok((ntype, count))
        })?;
        let (mut nat, mut bridge, mut vxlan) = (0, 0, 0);
        for row in rows {
            let (ntype, count) = row?;
            match ntype.as_str() {
                "nat" => nat = count,
                "bridge" => bridge = count,
                "vxlan" => vxlan = count,
                _ => {}
            }
        }
        Ok((nat, bridge, vxlan))
    }

    pub fn count_nodes_cert_expiry(&self) -> Result<(i32, i32), rusqlite::Error> {
        let conn = self.lock_conn()?;
        let expiring: i32 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE cert_expiry_days > 0 AND cert_expiry_days <= 30",
            [],
            |row| row.get(0),
        )?;
        let unknown: i32 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE cert_expiry_days < 0",
            [],
            |row| row.get(0),
        )?;
        Ok((expiring, unknown))
    }

    pub fn count_nodes_luks_method(&self) -> Result<(i32, i32, i32), rusqlite::Error> {
        let conn = self.lock_conn()?;
        let tpm2: i32 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE luks_method = 'tpm2'",
            [],
            |row| row.get(0),
        )?;
        let keyfile: i32 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE luks_method = 'key-file'",
            [],
            |row| row.get(0),
        )?;
        let unknown: i32 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE luks_method = '' OR luks_method IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok((tpm2, keyfile, unknown))
    }
}

fn row_to_node(row: &rusqlite::Row) -> Result<NodeRow, rusqlite::Error> {
    let disable_vxlan_int: i32 = row.get(11)?;
    Ok(NodeRow {
        id: row.get(0)?,
        hostname: row.get(1)?,
        address: row.get(2)?,
        cpu_cores: row.get(3)?,
        memory_bytes: row.get(4)?,
        status: row.get(5)?,
        last_heartbeat: row.get(6)?,
        gateway_interface: row.get(7)?,
        cpu_used: row.get(8)?,
        memory_used: row.get(9)?,
        storage_backend: row.get(10)?,
        disable_vxlan: disable_vxlan_int != 0,
        approval_status: row.get(12)?,
        cert_expiry_days: row.get(13)?,
        luks_method: row.get(14)?,
    })
}

fn row_to_vm(row: &rusqlite::Row) -> Result<VmRow, rusqlite::Error> {
    let image_path: String = row.get(4)?;
    let image_url: String = row.get(5)?;
    let image_format: String = row.get(7)?;
    Ok(VmRow {
        id: row.get(0)?,
        name: row.get(1)?,
        cpu: row.get(2)?,
        memory_bytes: row.get(3)?,
        image_path: image_path.clone(),
        image_url: image_url.clone(),
        image_sha256: row.get(6)?,
        image_format: normalize_image_format(&image_format, &image_path, &image_url),
        image_size: row.get(8)?,
        network: row.get(9)?,
        auto_start: row.get::<_, i32>(10)? != 0,
        node_id: row.get(11)?,
        created_at: row.get(12)?,
        runtime_state: row.get(13)?,
        cloud_init_user_data: row.get(14)?,
        storage_backend: row.get(15)?,
        storage_size_bytes: row.get(16)?,
        vm_ip: row.get(17)?,
    })
}

fn row_to_network(row: &rusqlite::Row) -> Result<NetworkRow, rusqlite::Error> {
    Ok(NetworkRow {
        name: row.get(0)?,
        external_ip: row.get(1)?,
        gateway_ip: row.get(2)?,
        internal_netmask: row.get(3)?,
        node_id: row.get(4)?,
        allowed_tcp_ports: row.get(5)?,
        allowed_udp_ports: row.get(6)?,
        vlan_id: row.get(7)?,
        network_type: row.get(8)?,
        enable_outbound_nat: row.get::<_, i32>(9)? != 0,
        vni: row.get(10)?,
        next_ip: row.get(11)?,
    })
}

fn normalize_image_format(format: &str, image_path: &str, image_url: &str) -> String {
    let normalized = format.trim().to_ascii_lowercase();
    match normalized.as_str() {
        // Legacy rows can end up with "raw" as a default after migration.
        // If they come from URL-backed images and the path clearly looks qcow2, correct it.
        "raw"
            if !image_url.is_empty()
                && matches!(infer_image_format_from_path(image_path), "qcow2") =>
        {
            "qcow2".to_string()
        }
        "raw" | "qcow2" => normalized,
        _ => infer_image_format_from_path(image_path).to_string(),
    }
}

fn infer_image_format_from_path(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".qcow2") || lower.ends_with(".qcow") {
        "qcow2"
    } else {
        "raw"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_node() -> NodeRow {
        NodeRow {
            id: "n1".to_string(),
            hostname: "n1".to_string(),
            address: "127.0.0.1:9091".to_string(),
            cpu_cores: 4,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            status: "ready".to_string(),
            last_heartbeat: String::new(),
            gateway_interface: "eno1".to_string(),
            cpu_used: 0,
            memory_used: 0,
            storage_backend: "filesystem".to_string(),
            disable_vxlan: false,
            approval_status: "approved".to_string(),
            cert_expiry_days: -1,
            luks_method: String::new(),
        }
    }

    fn test_vm(node_id: &str) -> VmRow {
        VmRow {
            id: "vm-1".to_string(),
            name: "web-1".to_string(),
            cpu: 2,
            memory_bytes: 2 * 1024 * 1024 * 1024,
            image_path: "/var/lib/kcore/images/web-1.raw".to_string(),
            image_url: String::new(),
            image_sha256: String::new(),
            image_format: "raw".to_string(),
            image_size: 8192,
            network: "default".to_string(),
            auto_start: true,
            node_id: node_id.to_string(),
            created_at: String::new(),
            runtime_state: "unknown".to_string(),
            cloud_init_user_data: String::new(),
            storage_backend: "filesystem".to_string(),
            storage_size_bytes: 0,
            vm_ip: String::new(),
        }
    }

    #[test]
    fn set_vm_auto_start_updates_by_name() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");
        db.insert_vm(&test_vm(&node.id)).expect("insert vm");

        let changed = db
            .set_vm_auto_start("web-1", false)
            .expect("update auto_start");
        assert!(changed);

        let updated = db.get_vm("vm-1").expect("get vm").expect("vm");
        assert!(!updated.auto_start);
    }

    #[test]
    fn migrate_infers_qcow2_for_legacy_url_rows() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");

        db.insert_vm(&VmRow {
            id: "vm-qcow".to_string(),
            name: "vm-qcow".to_string(),
            cpu: 1,
            memory_bytes: 1024 * 1024 * 1024,
            image_path: "/var/lib/kcore/images/debian.qcow2".to_string(),
            image_url: "https://example.com/debian.qcow2".to_string(),
            image_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            image_format: "raw".to_string(),
            image_size: 4096,
            network: "default".to_string(),
            auto_start: true,
            node_id: node.id.clone(),
            created_at: String::new(),
            runtime_state: "unknown".to_string(),
            cloud_init_user_data: String::new(),
            storage_backend: "filesystem".to_string(),
            storage_size_bytes: 0,
            vm_ip: String::new(),
        })
        .expect("insert qcow vm");

        let vm = db.get_vm("vm-qcow").expect("get vm").expect("vm exists");
        assert_eq!(vm.image_format, "qcow2");
    }

    #[test]
    fn network_roundtrip_works() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");
        db.insert_network(&NetworkRow {
            name: "frontend".to_string(),
            external_ip: "203.0.113.10".to_string(),
            gateway_ip: "10.240.10.1".to_string(),
            internal_netmask: "255.255.255.0".to_string(),
            node_id: node.id.clone(),
            allowed_tcp_ports: String::new(),
            allowed_udp_ports: String::new(),
            vlan_id: 0,
            network_type: "nat".to_string(),
            enable_outbound_nat: true,
            vni: 0,
            next_ip: 2,
        })
        .expect("insert network");

        let got = db
            .get_network_for_node(&node.id, "frontend")
            .expect("get network")
            .expect("network exists");
        assert_eq!(got.gateway_ip, "10.240.10.1");

        let listed = db
            .list_networks_for_node(&node.id)
            .expect("list networks for node");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "frontend");
    }

    #[test]
    fn node_storage_backend_roundtrip_works() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.storage_backend = "zfs".to_string();
        db.upsert_node(&node).expect("insert node");

        let got = db
            .get_node(&node.id)
            .expect("get node")
            .expect("node exists");
        assert_eq!(got.storage_backend, "zfs");
    }

    #[test]
    fn node_disable_vxlan_roundtrip() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.disable_vxlan = true;
        db.upsert_node(&node).expect("insert node");

        let got = db
            .get_node(&node.id)
            .expect("get node")
            .expect("node exists");
        assert!(got.disable_vxlan);

        node.disable_vxlan = false;
        db.upsert_node(&node).expect("update node");
        let got2 = db
            .get_node(&node.id)
            .expect("get node")
            .expect("node exists");
        assert!(!got2.disable_vxlan);
    }

    #[test]
    fn node_approval_status_roundtrip() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.approval_status = "pending".to_string();
        db.upsert_node(&node).expect("insert node");

        let got = db.get_node(&node.id).expect("get").expect("exists");
        assert_eq!(got.approval_status, "pending");

        db.set_node_approval(&node.id, "approved").expect("approve");
        let got2 = db.get_node(&node.id).expect("get").expect("exists");
        assert_eq!(got2.approval_status, "approved");

        db.set_node_approval(&node.id, "rejected").expect("reject");
        let got3 = db.get_node(&node.id).expect("get").expect("exists");
        assert_eq!(got3.approval_status, "rejected");
    }

    #[test]
    fn upsert_node_preserves_approval_status() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.approval_status = "pending".to_string();
        db.upsert_node(&node).expect("insert");

        db.set_node_approval(&node.id, "approved").expect("approve");

        node.hostname = "updated-hostname".to_string();
        node.approval_status = "pending".to_string();
        db.upsert_node(&node).expect("upsert again");

        let got = db.get_node(&node.id).expect("get").expect("exists");
        assert_eq!(got.hostname, "updated-hostname");
        assert_eq!(
            got.approval_status, "approved",
            "upsert should not overwrite approval_status"
        );
    }

    #[test]
    fn heartbeat_skips_non_approved_nodes() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.approval_status = "pending".to_string();
        node.status = "pending".to_string();
        db.upsert_node(&node).expect("insert");

        let updated = db
            .update_heartbeat(&node.id, 1, 1000, -1, "")
            .expect("heartbeat");
        assert!(!updated, "heartbeat should not update a non-approved node");

        let got = db.get_node(&node.id).expect("get").expect("exists");
        assert_eq!(got.status, "pending", "status should still be pending");
    }

    #[test]
    fn network_type_and_vxlan_fields_roundtrip() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");

        db.insert_network(&NetworkRow {
            name: "overlay".to_string(),
            external_ip: "0.0.0.0".to_string(),
            gateway_ip: "10.200.0.1".to_string(),
            internal_netmask: "255.255.255.0".to_string(),
            node_id: node.id.clone(),
            allowed_tcp_ports: String::new(),
            allowed_udp_ports: String::new(),
            vlan_id: 0,
            network_type: "vxlan".to_string(),
            enable_outbound_nat: false,
            vni: 10042,
            next_ip: 2,
        })
        .expect("insert vxlan network");

        let got = db
            .get_network_for_node(&node.id, "overlay")
            .expect("get")
            .expect("exists");
        assert_eq!(got.network_type, "vxlan");
        assert!(!got.enable_outbound_nat);
        assert_eq!(got.vni, 10042);
        assert_eq!(got.next_ip, 2);
    }

    #[test]
    fn list_networks_by_name_returns_all_nodes() {
        let db = Database::open(":memory:").expect("open db");
        let mut n1 = test_node();
        n1.id = "n1".to_string();
        let mut n2 = test_node();
        n2.id = "n2".to_string();
        db.upsert_node(&n1).expect("insert n1");
        db.upsert_node(&n2).expect("insert n2");

        for nid in ["n1", "n2"] {
            db.insert_network(&NetworkRow {
                name: "shared".to_string(),
                external_ip: "0.0.0.0".to_string(),
                gateway_ip: "10.200.0.1".to_string(),
                internal_netmask: "255.255.255.0".to_string(),
                node_id: nid.to_string(),
                allowed_tcp_ports: String::new(),
                allowed_udp_ports: String::new(),
                vlan_id: 0,
                network_type: "vxlan".to_string(),
                enable_outbound_nat: true,
                vni: 10100,
                next_ip: 2,
            })
            .expect("insert network");
        }

        let all = db.list_networks_by_name("shared").expect("list");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn allocate_vm_ip_increments_sequentially() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");

        db.insert_network(&NetworkRow {
            name: "vxnet".to_string(),
            external_ip: "0.0.0.0".to_string(),
            gateway_ip: "10.200.0.1".to_string(),
            internal_netmask: "255.255.255.0".to_string(),
            node_id: node.id.clone(),
            allowed_tcp_ports: String::new(),
            allowed_udp_ports: String::new(),
            vlan_id: 0,
            network_type: "vxlan".to_string(),
            enable_outbound_nat: true,
            vni: 10050,
            next_ip: 2,
        })
        .expect("insert network");

        let ip1 = db.allocate_vm_ip("vxnet", &node.id).expect("alloc 1");
        assert_eq!(ip1, "10.200.0.2");

        let ip2 = db.allocate_vm_ip("vxnet", &node.id).expect("alloc 2");
        assert_eq!(ip2, "10.200.0.3");

        let ip3 = db.allocate_vm_ip("vxnet", &node.id).expect("alloc 3");
        assert_eq!(ip3, "10.200.0.4");
    }

    #[test]
    fn vm_ip_stored_and_retrieved() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");

        let mut vm = test_vm(&node.id);
        vm.vm_ip = "10.200.0.5".to_string();
        db.insert_vm(&vm).expect("insert vm");

        let got = db.get_vm("vm-1").expect("get").expect("exists");
        assert_eq!(got.vm_ip, "10.200.0.5");
    }

    #[test]
    fn replication_outbox_append_and_count() {
        let db = Database::open(":memory:").expect("open db");
        assert_eq!(db.replication_outbox_len().expect("count"), 0);
        let id = db
            .append_replication_outbox("node.register", "node/n1", br#"{"x":1}"#)
            .expect("append");
        assert!(id >= 1);
        assert_eq!(db.replication_outbox_len().expect("count"), 1);
    }

    #[test]
    fn replication_outbox_list_since_orders_and_limits() {
        let db = Database::open(":memory:").expect("open db");
        let id1 = db
            .append_replication_outbox("node.register", "node/n1", br#"{"seq":1}"#)
            .expect("append 1");
        let _id2 = db
            .append_replication_outbox("vm.create", "vm/v1", br#"{"seq":2}"#)
            .expect("append 2");

        let rows = db
            .list_replication_outbox_since(id1, 10)
            .expect("list since id1");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].event_type, "vm.create");
        assert_eq!(rows[0].resource_key, "vm/v1");
        assert_eq!(rows[0].payload, br#"{"seq":2}"#);

        let all = db
            .list_replication_outbox_since(0, 1)
            .expect("list limited");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, id1);
    }

    #[test]
    fn replication_ack_upsert_and_read() {
        let db = Database::open(":memory:").expect("open db");
        assert_eq!(db.get_replication_ack("peer-a").expect("get"), None);

        db.upsert_replication_ack("peer-a", 42)
            .expect("upsert first");
        assert_eq!(db.get_replication_ack("peer-a").expect("get"), Some(42));

        db.upsert_replication_ack("peer-a", 105)
            .expect("upsert second");
        assert_eq!(db.get_replication_ack("peer-a").expect("get"), Some(105));
    }

    #[test]
    fn replication_outbox_head_id_empty_and_populated() {
        let db = Database::open(":memory:").expect("open db");
        assert_eq!(db.replication_outbox_head_id().expect("head"), 0);
        let id = db
            .append_replication_outbox("node.register", "node/n1", br#"{"x":1}"#)
            .expect("append");
        assert_eq!(db.replication_outbox_head_id().expect("head"), id);
    }

    #[test]
    fn replication_ack_list_returns_rows() {
        let db = Database::open(":memory:").expect("open db");
        db.upsert_replication_ack("peer-a", 7).expect("upsert a");
        db.upsert_replication_ack("pull/10.0.0.11:9090", 3)
            .expect("upsert pull");
        let rows = db.list_replication_acks().expect("list");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].peer_id, "peer-a");
        assert_eq!(rows[0].last_event_id, 7);
        assert!(!rows[0].updated_at.is_empty());
    }

    #[test]
    fn replication_received_op_insert_and_exists() {
        let db = Database::open(":memory:").expect("open db");
        assert!(!db
            .replication_received_op_exists("op-1")
            .expect("exists before"));
        db.insert_replication_received_op("op-1", "ctrl-a", "vm.create", "vm/v1")
            .expect("insert");
        assert!(db
            .replication_received_op_exists("op-1")
            .expect("exists after"));
    }

    #[test]
    fn replication_resource_head_roundtrip() {
        let db = Database::open(":memory:").expect("open db");
        let row = ReplicationResourceHeadRow {
            resource_key: "vm/v1".into(),
            last_op_id: "op-1".into(),
            last_logical_ts_unix_ms: 123,
            last_policy_priority: 10,
            last_intent_epoch: 3,
            last_validity: "valid".into(),
            last_safety_class: "safe".into(),
            last_controller_id: "ctrl-a".into(),
            last_event_id: 7,
            last_event_type: "vm.update".into(),
            last_body_json: "{\"cpu\":2}".into(),
        };
        db.upsert_replication_resource_head(&row).expect("upsert");
        let got = db
            .get_replication_resource_head("vm/v1")
            .expect("get")
            .expect("exists");
        assert_eq!(got.last_op_id, "op-1");
        assert_eq!(got.last_logical_ts_unix_ms, 123);
        assert_eq!(got.last_policy_priority, 10);
        assert_eq!(got.last_intent_epoch, 3);
        assert_eq!(got.last_event_type, "vm.update");
    }

    #[test]
    fn replication_conflict_insert_and_query() {
        let db = Database::open(":memory:").expect("open db");
        let id = db
            .insert_replication_conflict(
                "vm/v1",
                "op-old",
                "op-new",
                "ctrl-a",
                "ctrl-b",
                "same logical timestamp",
            )
            .expect("insert conflict");
        assert!(id >= 1);
        assert_eq!(
            db.count_unresolved_replication_conflicts()
                .expect("count conflicts"),
            1
        );
        let rows = db
            .list_unresolved_replication_conflicts(10)
            .expect("list conflicts");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].resource_key, "vm/v1");
        assert_eq!(rows[0].challenger_controller_id, "ctrl-b");
        assert!(db
            .resolve_replication_conflict(rows[0].id)
            .expect("resolve conflict"));
        assert_eq!(
            db.count_unresolved_replication_conflicts()
                .expect("count after resolve"),
            0
        );
    }

    #[test]
    fn compensation_job_roundtrip() {
        let db = Database::open(":memory:").expect("open db");
        let conflict_id = db
            .insert_replication_conflict(
                "vm/v1",
                "op-inc",
                "op-loser",
                "ctrl-a",
                "ctrl-b",
                "needs compensation",
            )
            .expect("insert conflict");
        assert_eq!(db.count_pending_compensation_jobs().expect("count"), 0);
        let job_id = db
            .insert_compensation_job(conflict_id, "vm/v1", "op-loser")
            .expect("insert job");
        assert!(job_id >= 1);
        assert_eq!(db.count_pending_compensation_jobs().expect("count"), 1);
        let job = db
            .claim_next_compensation_job()
            .expect("claim")
            .expect("job exists");
        assert_eq!(job.status, "running");
        assert_eq!(job.conflict_id, conflict_id);
        assert_eq!(
            db.get_compensation_job_status_for_loser_op("op-loser")
                .expect("status lookup"),
            Some("running".to_string())
        );
        db.complete_compensation_job(job.id).expect("complete");
        assert_eq!(db.count_pending_compensation_jobs().expect("count"), 0);
        assert_eq!(
            db.get_compensation_job_status_for_loser_op("op-loser")
                .expect("status lookup"),
            Some("completed".to_string())
        );
    }

    #[test]
    fn materialized_head_roundtrip() {
        let db = Database::open(":memory:").expect("open db");
        assert!(db
            .get_materialized_replication_head("vm/v1")
            .expect("get")
            .is_none());
        db.upsert_materialized_replication_head("vm/v1", "op-1", "vm.update")
            .expect("upsert");
        let row = db
            .get_materialized_replication_head("vm/v1")
            .expect("get")
            .expect("present");
        assert_eq!(row.resource_key, "vm/v1");
        assert_eq!(row.last_op_id, "op-1");
        assert_eq!(row.last_event_type, "vm.update");
    }

    #[test]
    fn replication_reservation_roundtrip() {
        let db = Database::open(":memory:").expect("open db");
        db.upsert_replication_reservation("node-capacity/node-1", "vm/v1", "op-1", "reserved", "")
            .expect("upsert reservation");
        let row = db
            .get_replication_reservation("node-capacity/node-1", "vm/v1")
            .expect("get reservation")
            .expect("reservation present");
        assert_eq!(row.op_id, "op-1");
        assert_eq!(row.status, "reserved");
        assert!(row.error.is_empty());
        assert_eq!(row.retry_count, 0);
    }

    #[test]
    fn replication_reservation_failure_classification_and_budget() {
        let db = Database::open(":memory:").expect("open db");
        let (status1, retry1) = db
            .record_replication_reservation_failure(
                "node-capacity/node-x",
                "vm/v1",
                "op-1",
                true,
                "node not ready",
                3,
            )
            .expect("failure 1");
        assert_eq!(status1, "failed_retryable");
        assert_eq!(retry1, 1);
        let (status2, retry2) = db
            .record_replication_reservation_failure(
                "node-capacity/node-x",
                "vm/v1",
                "op-1",
                true,
                "node not ready",
                3,
            )
            .expect("failure 2");
        assert_eq!(status2, "failed_retryable");
        assert_eq!(retry2, 2);
        let (status3, retry3) = db
            .record_replication_reservation_failure(
                "node-capacity/node-x",
                "vm/v1",
                "op-1",
                true,
                "node not ready",
                3,
            )
            .expect("failure 3");
        assert_eq!(status3, "retry_exhausted");
        assert_eq!(retry3, 3);
        assert_eq!(
            db.count_failed_retryable_replication_reservations()
                .expect("count retryable"),
            0
        );
        assert_eq!(
            db.count_failed_replication_reservations()
                .expect("count failed total"),
            1
        );
        assert_eq!(
            db.count_retry_exhausted_replication_reservations()
                .expect("count exhausted"),
            1
        );
    }

    #[test]
    fn list_retryable_replication_reservations_returns_only_retryable() {
        let db = Database::open(":memory:").expect("open db");
        db.upsert_replication_reservation_with_retry(
            "node-capacity/node-a",
            "vm/v-a",
            "op-a",
            "failed_retryable",
            "node not ready",
            1,
        )
        .expect("insert retryable");
        db.upsert_replication_reservation_with_retry(
            "node-capacity/node-b",
            "vm/v-b",
            "op-b",
            "failed_non_retryable",
            "node missing",
            1,
        )
        .expect("insert non-retryable");
        let rows = db
            .list_retryable_replication_reservations(10, 0)
            .expect("list retryable");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].resource_key, "vm/v-a");
    }

    #[test]
    fn replication_metrics_queries_work() {
        let db = Database::open(":memory:").expect("open db");
        db.insert_replication_conflict("vm/v1", "op-a", "op-b", "ctrl-a", "ctrl-b", "conflict")
            .expect("insert conflict");
        db.upsert_replication_reservation(
            "node-capacity/node-1",
            "vm/v1",
            "op-1",
            "failed_non_retryable",
            "x",
        )
        .expect("insert failed reservation");
        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "vm/v1".to_string(),
            last_op_id: "op-1".to_string(),
            last_logical_ts_unix_ms: 1,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 1,
            last_event_type: "vm.update".to_string(),
            last_body_json: "{}".to_string(),
        })
        .expect("insert head");
        db.upsert_materialized_replication_head("vm/v1", "op-0", "vm.update")
            .expect("insert stale materialized head");

        assert_eq!(
            db.count_failed_replication_reservations()
                .expect("count failed reservations"),
            1
        );
        assert_eq!(
            db.count_replication_materialization_backlog()
                .expect("materialization backlog"),
            1
        );
        assert!(
            db.oldest_unresolved_conflict_age_seconds()
                .expect("oldest unresolved age")
                >= 0
        );
    }
}
