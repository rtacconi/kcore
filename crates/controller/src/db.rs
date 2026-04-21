use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

fn is_sqlite_memory_database_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    if lower == ":memory:" {
        return true;
    }
    if lower.starts_with("file::memory:") {
        return true;
    }
    if let Some(q) = lower.find('?') {
        if lower[q..].contains("mode=memory") {
            return true;
        }
    }
    false
}

fn validate_database_path(path: &str) -> Result<()> {
    crate::path_safety::assert_safe_path(path, "database path")
}

#[derive(Debug, Clone)]
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
    pub dc_id: String,
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
pub struct WorkloadRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub node_id: String,
    pub runtime_state: String,
    pub desired_state: String,
    pub vm_id: String,
    pub container_image: String,
    pub network: String,
    pub storage_backend: String,
    pub storage_size_bytes: i64,
    pub created_at: String,
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
pub struct SecurityGroupRow {
    pub name: String,
    pub description: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct SecurityGroupRuleRow {
    pub id: String,
    pub security_group: String,
    pub protocol: String,
    pub host_port: i32,
    pub target_port: i32,
    pub source_cidr: String,
    pub target_vm: String,
    pub enable_dnat: bool,
}

#[derive(Debug, Clone)]
pub struct SecurityGroupVmAttachmentRow {
    pub security_group: String,
    pub vm_id: String,
}

#[derive(Debug, Clone)]
pub struct DiskLayoutRow {
    pub name: String,
    pub node_id: String,
    pub generation: i64,
    pub layout_nix: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct DiskLayoutStatusRow {
    pub name: String,
    pub observed_generation: i64,
    pub phase: String,
    pub refusal_reason: String,
    pub message: String,
    pub last_transition_at: String,
}

#[derive(Debug, Clone)]
pub struct SecurityGroupNetworkAttachmentRow {
    pub security_group: String,
    pub network_name: String,
    pub node_id: String,
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
    pub loser_event_type: String,
    pub loser_body_json: String,
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

#[derive(Debug, Clone)]
pub struct ControllerPeerRow {
    pub controller_id: String,
    pub address: String,
    pub dc_id: String,
    pub last_seen_at: String,
}

impl Database {
    pub fn open(path: &str) -> Result<Self> {
        validate_database_path(path)?;
        if !is_sqlite_memory_database_path(path) {
            if let Some(parent) = std::path::Path::new(path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).with_context(|| {
                        format!(
                            "failed to create database parent directory {}",
                            parent.display()
                        )
                    })?;
                }
            }
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
            );
            CREATE TABLE IF NOT EXISTS workloads (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                node_id TEXT NOT NULL REFERENCES nodes(id),
                runtime_state TEXT NOT NULL DEFAULT 'unknown',
                desired_state TEXT NOT NULL DEFAULT 'running',
                vm_id TEXT NOT NULL DEFAULT '',
                container_image TEXT NOT NULL DEFAULT '',
                network TEXT NOT NULL DEFAULT 'default',
                storage_backend TEXT NOT NULL DEFAULT 'filesystem',
                storage_size_bytes INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS security_groups (
                name TEXT PRIMARY KEY,
                description TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS security_group_rules (
                id TEXT PRIMARY KEY,
                security_group TEXT NOT NULL REFERENCES security_groups(name) ON DELETE CASCADE,
                protocol TEXT NOT NULL,
                host_port INTEGER NOT NULL,
                target_port INTEGER NOT NULL,
                source_cidr TEXT NOT NULL DEFAULT '',
                target_vm TEXT NOT NULL DEFAULT '',
                enable_dnat INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS security_group_vm_attachments (
                security_group TEXT NOT NULL REFERENCES security_groups(name) ON DELETE CASCADE,
                vm_id TEXT NOT NULL REFERENCES vms(id) ON DELETE CASCADE,
                PRIMARY KEY (security_group, vm_id)
            );
            CREATE TABLE IF NOT EXISTS security_group_network_attachments (
                security_group TEXT NOT NULL REFERENCES security_groups(name) ON DELETE CASCADE,
                network_name TEXT NOT NULL,
                node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                PRIMARY KEY (security_group, network_name, node_id)
            );
            CREATE TABLE IF NOT EXISTS disk_layouts (
                name TEXT PRIMARY KEY,
                node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                generation INTEGER NOT NULL DEFAULT 1,
                layout_nix TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_disk_layouts_node ON disk_layouts(node_id);
            CREATE TABLE IF NOT EXISTS disk_layout_status (
                name TEXT PRIMARY KEY REFERENCES disk_layouts(name) ON DELETE CASCADE,
                observed_generation INTEGER NOT NULL DEFAULT 0,
                phase TEXT NOT NULL DEFAULT 'pending',
                refusal_reason TEXT NOT NULL DEFAULT '',
                message TEXT NOT NULL DEFAULT '',
                last_transition_at TEXT NOT NULL DEFAULT (datetime('now'))
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

        if version < 22 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS workloads (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    node_id TEXT NOT NULL REFERENCES nodes(id),
                    runtime_state TEXT NOT NULL DEFAULT 'unknown',
                    desired_state TEXT NOT NULL DEFAULT 'running',
                    vm_id TEXT NOT NULL DEFAULT '',
                    container_image TEXT NOT NULL DEFAULT '',
                    network TEXT NOT NULL DEFAULT 'default',
                    storage_backend TEXT NOT NULL DEFAULT 'filesystem',
                    storage_size_bytes INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );
                CREATE INDEX IF NOT EXISTS idx_workloads_node_id ON workloads(node_id);
                CREATE INDEX IF NOT EXISTS idx_workloads_kind ON workloads(kind);
                CREATE INDEX IF NOT EXISTS idx_workloads_runtime_state ON workloads(runtime_state);",
            );
        }
        if version < 23 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS security_groups (
                    name TEXT PRIMARY KEY,
                    description TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );
                CREATE TABLE IF NOT EXISTS security_group_rules (
                    id TEXT PRIMARY KEY,
                    security_group TEXT NOT NULL REFERENCES security_groups(name) ON DELETE CASCADE,
                    protocol TEXT NOT NULL,
                    host_port INTEGER NOT NULL,
                    target_port INTEGER NOT NULL,
                    source_cidr TEXT NOT NULL DEFAULT '',
                    target_vm TEXT NOT NULL DEFAULT '',
                    enable_dnat INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS security_group_vm_attachments (
                    security_group TEXT NOT NULL REFERENCES security_groups(name) ON DELETE CASCADE,
                    vm_id TEXT NOT NULL REFERENCES vms(id) ON DELETE CASCADE,
                    PRIMARY KEY (security_group, vm_id)
                );
                CREATE TABLE IF NOT EXISTS security_group_network_attachments (
                    security_group TEXT NOT NULL REFERENCES security_groups(name) ON DELETE CASCADE,
                    network_name TEXT NOT NULL,
                    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                    PRIMARY KEY (security_group, network_name, node_id)
                );",
            );
        }

        if version < 24 {
            let _ = conn.execute(
                "ALTER TABLE replication_compensation_jobs ADD COLUMN loser_event_type TEXT NOT NULL DEFAULT ''",
                [],
            );
            let _ = conn.execute(
                "ALTER TABLE replication_compensation_jobs ADD COLUMN loser_body_json TEXT NOT NULL DEFAULT '{}'",
                [],
            );
        }

        if version < 25 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS controller_peers (
                    controller_id TEXT PRIMARY KEY,
                    address TEXT NOT NULL,
                    dc_id TEXT NOT NULL DEFAULT 'DC1',
                    last_seen_at TEXT NOT NULL DEFAULT (datetime('now'))
                );",
            );
        }

        if version < 26 {
            let _ = conn.execute(
                "ALTER TABLE nodes ADD COLUMN dc_id TEXT NOT NULL DEFAULT ''",
                [],
            );
        }

        if version < 27 {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS disk_layouts (
                    name TEXT PRIMARY KEY,
                    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                    generation INTEGER NOT NULL DEFAULT 1,
                    layout_nix TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
                );
                CREATE INDEX IF NOT EXISTS idx_disk_layouts_node ON disk_layouts(node_id);
                CREATE TABLE IF NOT EXISTS disk_layout_status (
                    name TEXT PRIMARY KEY REFERENCES disk_layouts(name) ON DELETE CASCADE,
                    observed_generation INTEGER NOT NULL DEFAULT 0,
                    phase TEXT NOT NULL DEFAULT 'pending',
                    refusal_reason TEXT NOT NULL DEFAULT '',
                    message TEXT NOT NULL DEFAULT '',
                    last_transition_at TEXT NOT NULL DEFAULT (datetime('now'))
                );",
            );
        }

        const CURRENT_VERSION: i32 = 27;
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
               last_event_id=MAX(replication_ack.last_event_id, excluded.last_event_id),
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

    pub fn upsert_controller_peer(
        &self,
        controller_id: &str,
        address: &str,
        dc_id: &str,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO controller_peers (controller_id, address, dc_id, last_seen_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(controller_id) DO UPDATE SET
                address = excluded.address,
                dc_id = excluded.dc_id,
                last_seen_at = datetime('now')",
            params![controller_id, address, dc_id],
        )?;
        Ok(())
    }

    /// Like `upsert_controller_peer` but only updates `address` and
    /// `last_seen_at`, leaving `dc_id` unchanged. Used by the ack handler
    /// so that a receiver doesn't clobber the peer's real DC identity
    /// (which is set authoritatively by `controller.register` materialization).
    pub fn upsert_controller_peer_address_only(
        &self,
        controller_id: &str,
        address: &str,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO controller_peers (controller_id, address, dc_id, last_seen_at)
             VALUES (?1, ?2, 'DC1', datetime('now'))
             ON CONFLICT(controller_id) DO UPDATE SET
                address = excluded.address,
                last_seen_at = datetime('now')",
            params![controller_id, address],
        )?;
        Ok(())
    }

    pub fn list_controller_peers(&self) -> Result<Vec<ControllerPeerRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT controller_id, address, dc_id, last_seen_at
             FROM controller_peers
             ORDER BY controller_id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ControllerPeerRow {
                controller_id: row.get(0)?,
                address: row.get(1)?,
                dc_id: row.get(2)?,
                last_seen_at: row.get(3)?,
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
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(op_id) DO NOTHING",
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
        loser_event_type: &str,
        loser_body_json: &str,
    ) -> Result<i64, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO replication_compensation_jobs (conflict_id, resource_key, loser_op_id, loser_event_type, loser_body_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                conflict_id,
                resource_key,
                loser_op_id,
                loser_event_type,
                loser_body_json
            ],
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
            "SELECT id, conflict_id, resource_key, loser_op_id, loser_event_type, loser_body_json, status, attempts
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
                loser_event_type: row.get(4)?,
                loser_body_json: row.get(5)?,
                status: row.get(6)?,
                attempts: row.get(7)?,
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
            "INSERT INTO nodes (id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface, cpu_used, memory_used, storage_backend, disable_vxlan, approval_status, cert_expiry_days, luks_method, dc_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
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
                luks_method=excluded.luks_method,
                dc_id=excluded.dc_id",
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
                node.dc_id,
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
            "UPDATE nodes SET last_heartbeat = datetime('now'), \
             status = CASE WHEN approval_status = 'approved' THEN 'ready' ELSE status END, \
             cpu_used = ?2, memory_used = ?3, cert_expiry_days = ?4, luks_method = ?5 \
             WHERE id = ?1",
            params![node_id, cpu_used, mem_used, cert_expiry_days, luks_method],
        )?;
        Ok(rows > 0)
    }

    pub fn get_node(&self, node_id: &str) -> Result<Option<NodeRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface, cpu_used, memory_used, storage_backend, disable_vxlan, approval_status, cert_expiry_days, luks_method, dc_id FROM nodes WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![node_id], row_to_node)?;
        rows.next().transpose()
    }

    pub fn list_nodes(&self) -> Result<Vec<NodeRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface, cpu_used, memory_used, storage_backend, disable_vxlan, approval_status, cert_expiry_days, luks_method, dc_id FROM nodes",
        )?;
        let rows = stmt.query_map([], row_to_node)?;
        rows.collect()
    }

    pub fn get_node_by_address(&self, address: &str) -> Result<Option<NodeRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface, cpu_used, memory_used, storage_backend, disable_vxlan, approval_status, cert_expiry_days, luks_method, dc_id FROM nodes WHERE address = ?1",
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

    pub fn upsert_workload(&self, workload: &WorkloadRow) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO workloads (
                id, name, kind, node_id, runtime_state, desired_state, vm_id, container_image,
                network, storage_backend, storage_size_bytes, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name,
                kind=excluded.kind,
                node_id=excluded.node_id,
                runtime_state=excluded.runtime_state,
                desired_state=excluded.desired_state,
                vm_id=excluded.vm_id,
                container_image=excluded.container_image,
                network=excluded.network,
                storage_backend=excluded.storage_backend,
                storage_size_bytes=excluded.storage_size_bytes",
            params![
                workload.id,
                workload.name,
                workload.kind,
                workload.node_id,
                workload.runtime_state,
                workload.desired_state,
                workload.vm_id,
                workload.container_image,
                workload.network,
                workload.storage_backend,
                workload.storage_size_bytes,
            ],
        )?;
        Ok(())
    }

    pub fn get_workload(
        &self,
        workload_id_or_name: &str,
    ) -> Result<Option<WorkloadRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, node_id, runtime_state, desired_state, vm_id, container_image, network, storage_backend, storage_size_bytes, created_at
             FROM workloads
             WHERE id = ?1 OR name = ?1
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![workload_id_or_name], row_to_workload)?;
        rows.next().transpose()
    }

    pub fn list_workloads(
        &self,
        kind_filter: Option<&str>,
        node_filter: Option<&str>,
    ) -> Result<Vec<WorkloadRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        match (kind_filter, node_filter) {
            (Some(kind), Some(node)) => {
                let mut stmt = conn.prepare(
                    "SELECT id, name, kind, node_id, runtime_state, desired_state, vm_id, container_image, network, storage_backend, storage_size_bytes, created_at
                     FROM workloads
                     WHERE kind = ?1 AND node_id = ?2
                     ORDER BY created_at ASC",
                )?;
                let rows = stmt.query_map(params![kind, node], row_to_workload)?;
                rows.collect()
            }
            (Some(kind), None) => {
                let mut stmt = conn.prepare(
                    "SELECT id, name, kind, node_id, runtime_state, desired_state, vm_id, container_image, network, storage_backend, storage_size_bytes, created_at
                     FROM workloads
                     WHERE kind = ?1
                     ORDER BY created_at ASC",
                )?;
                let rows = stmt.query_map(params![kind], row_to_workload)?;
                rows.collect()
            }
            (None, Some(node)) => {
                let mut stmt = conn.prepare(
                    "SELECT id, name, kind, node_id, runtime_state, desired_state, vm_id, container_image, network, storage_backend, storage_size_bytes, created_at
                     FROM workloads
                     WHERE node_id = ?1
                     ORDER BY created_at ASC",
                )?;
                let rows = stmt.query_map(params![node], row_to_workload)?;
                rows.collect()
            }
            (None, None) => {
                let mut stmt = conn.prepare(
                    "SELECT id, name, kind, node_id, runtime_state, desired_state, vm_id, container_image, network, storage_backend, storage_size_bytes, created_at
                     FROM workloads
                     ORDER BY created_at ASC",
                )?;
                let rows = stmt.query_map([], row_to_workload)?;
                rows.collect()
            }
        }
    }

    pub fn update_workload_runtime_state(
        &self,
        id_or_name: &str,
        runtime_state: &str,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "UPDATE workloads
             SET runtime_state = ?2
             WHERE id = ?1 OR name = ?1",
            params![id_or_name, runtime_state],
        )?;
        Ok(rows > 0)
    }

    pub fn update_workload_desired_state(
        &self,
        id_or_name: &str,
        desired_state: &str,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "UPDATE workloads
             SET desired_state = ?2
             WHERE id = ?1 OR name = ?1",
            params![id_or_name, desired_state],
        )?;
        Ok(rows > 0)
    }

    pub fn delete_workload_by_id_or_name(&self, id_or_name: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "DELETE FROM workloads WHERE id = ?1 OR name = ?1",
            params![id_or_name],
        )?;
        Ok(rows > 0)
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

    /// Allocate a VM IP that is unique across *all* nodes sharing this
    /// network name (used for VXLAN overlays where every node is in the
    /// same L2 domain). Picks the global max `next_ip`, returns it, and
    /// bumps every row's counter so the next call is also unique.
    pub fn allocate_vm_ip_global(&self, network_name: &str) -> Result<String, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let (gateway_ip, global_next): (String, i32) = conn.query_row(
            "SELECT gateway_ip, MAX(next_ip) FROM networks WHERE name = ?1",
            params![network_name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let new_next = global_next + 1;
        conn.execute(
            "UPDATE networks SET next_ip = ?1 WHERE name = ?2",
            params![new_next, network_name],
        )?;
        let prefix = gateway_ip.rsplit_once('.').map(|x| x.0).unwrap_or("10.0.0");
        Ok(format!("{}.{}", prefix, global_next))
    }

    pub fn delete_network(&self, node_id: &str, name: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "DELETE FROM networks WHERE node_id = ?1 AND name = ?2",
            params![node_id, name],
        )?;
        Ok(rows > 0)
    }

    pub fn upsert_security_group(&self, sg: &SecurityGroupRow) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO security_groups (name, description)
             VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET description=excluded.description",
            params![sg.name, sg.description],
        )?;
        Ok(())
    }

    pub fn get_security_group(
        &self,
        name: &str,
    ) -> Result<Option<SecurityGroupRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare("SELECT name, description, created_at FROM security_groups WHERE name = ?1")?;
        let mut rows = stmt.query_map(params![name], |row| {
            Ok(SecurityGroupRow {
                name: row.get(0)?,
                description: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        rows.next().transpose()
    }

    pub fn list_security_groups(&self) -> Result<Vec<SecurityGroupRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT name, description, created_at FROM security_groups ORDER BY name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SecurityGroupRow {
                name: row.get(0)?,
                description: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn delete_security_group(&self, name: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute("DELETE FROM security_groups WHERE name = ?1", params![name])?;
        Ok(rows > 0)
    }

    pub fn upsert_disk_layout(
        &self,
        layout: &DiskLayoutRow,
    ) -> Result<DiskLayoutRow, rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO disk_layouts (name, node_id, generation, layout_nix)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET
               node_id = excluded.node_id,
               generation = excluded.generation,
               layout_nix = excluded.layout_nix,
               updated_at = datetime('now')",
            params![
                layout.name,
                layout.node_id,
                layout.generation,
                layout.layout_nix,
            ],
        )?;
        let mut stmt = conn.prepare(
            "SELECT name, node_id, generation, layout_nix, created_at, updated_at
             FROM disk_layouts WHERE name = ?1",
        )?;
        let row = stmt.query_row(params![layout.name], |row| {
            Ok(DiskLayoutRow {
                name: row.get(0)?,
                node_id: row.get(1)?,
                generation: row.get(2)?,
                layout_nix: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        Ok(row)
    }

    pub fn get_disk_layout(&self, name: &str) -> Result<Option<DiskLayoutRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT name, node_id, generation, layout_nix, created_at, updated_at
             FROM disk_layouts WHERE name = ?1",
        )?;
        let mut rows = stmt.query_map(params![name], |row| {
            Ok(DiskLayoutRow {
                name: row.get(0)?,
                node_id: row.get(1)?,
                generation: row.get(2)?,
                layout_nix: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        rows.next().transpose()
    }

    pub fn list_disk_layouts(
        &self,
        node_id_filter: Option<&str>,
    ) -> Result<Vec<DiskLayoutRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        if let Some(node_id) = node_id_filter {
            let mut stmt = conn.prepare(
                "SELECT name, node_id, generation, layout_nix, created_at, updated_at
                 FROM disk_layouts WHERE node_id = ?1 ORDER BY name ASC",
            )?;
            let rows = stmt.query_map(params![node_id], |row| {
                Ok(DiskLayoutRow {
                    name: row.get(0)?,
                    node_id: row.get(1)?,
                    generation: row.get(2)?,
                    layout_nix: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?;
            rows.collect()
        } else {
            let mut stmt = conn.prepare(
                "SELECT name, node_id, generation, layout_nix, created_at, updated_at
                 FROM disk_layouts ORDER BY name ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(DiskLayoutRow {
                    name: row.get(0)?,
                    node_id: row.get(1)?,
                    generation: row.get(2)?,
                    layout_nix: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?;
            rows.collect()
        }
    }

    pub fn delete_disk_layout(&self, name: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute("DELETE FROM disk_layouts WHERE name = ?1", params![name])?;
        Ok(rows > 0)
    }

    pub fn upsert_disk_layout_status(
        &self,
        status: &DiskLayoutStatusRow,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO disk_layout_status (
                name, observed_generation, phase, refusal_reason, message, last_transition_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(name) DO UPDATE SET
               observed_generation = excluded.observed_generation,
               phase = excluded.phase,
               refusal_reason = excluded.refusal_reason,
               message = excluded.message,
               last_transition_at = datetime('now')",
            params![
                status.name,
                status.observed_generation,
                status.phase,
                status.refusal_reason,
                status.message,
            ],
        )?;
        Ok(())
    }

    pub fn get_disk_layout_status(
        &self,
        name: &str,
    ) -> Result<Option<DiskLayoutStatusRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT name, observed_generation, phase, refusal_reason, message, last_transition_at
             FROM disk_layout_status WHERE name = ?1",
        )?;
        let mut rows = stmt.query_map(params![name], |row| {
            Ok(DiskLayoutStatusRow {
                name: row.get(0)?,
                observed_generation: row.get(1)?,
                phase: row.get(2)?,
                refusal_reason: row.get(3)?,
                message: row.get(4)?,
                last_transition_at: row.get(5)?,
            })
        })?;
        rows.next().transpose()
    }

    pub fn list_disk_layout_statuses(&self) -> Result<Vec<DiskLayoutStatusRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT name, observed_generation, phase, refusal_reason, message, last_transition_at
             FROM disk_layout_status",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DiskLayoutStatusRow {
                name: row.get(0)?,
                observed_generation: row.get(1)?,
                phase: row.get(2)?,
                refusal_reason: row.get(3)?,
                message: row.get(4)?,
                last_transition_at: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Return disk layouts whose status has `observed_generation < generation`
    /// (or no status row yet). These are the rows the reconciler should push
    /// to the node-agent on the next tick.
    pub fn list_disk_layouts_needing_reconcile(
        &self,
    ) -> Result<Vec<DiskLayoutRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT d.name, d.node_id, d.generation, d.layout_nix, d.created_at, d.updated_at
             FROM disk_layouts d
             LEFT JOIN disk_layout_status s ON s.name = d.name
             WHERE s.name IS NULL OR s.observed_generation < d.generation
             ORDER BY d.name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DiskLayoutRow {
                name: row.get(0)?,
                node_id: row.get(1)?,
                generation: row.get(2)?,
                layout_nix: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn replace_security_group_rules(
        &self,
        security_group: &str,
        rules: &[SecurityGroupRuleRow],
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM security_group_rules WHERE security_group = ?1",
            params![security_group],
        )?;
        for rule in rules {
            tx.execute(
                "INSERT INTO security_group_rules (id, security_group, protocol, host_port, target_port, source_cidr, target_vm, enable_dnat)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    rule.id,
                    security_group,
                    rule.protocol,
                    rule.host_port,
                    rule.target_port,
                    rule.source_cidr,
                    rule.target_vm,
                    rule.enable_dnat as i32
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_security_group_rules(
        &self,
        security_group: &str,
    ) -> Result<Vec<SecurityGroupRuleRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, security_group, protocol, host_port, target_port, source_cidr, target_vm, enable_dnat
             FROM security_group_rules
             WHERE security_group = ?1
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![security_group], |row| {
            Ok(SecurityGroupRuleRow {
                id: row.get(0)?,
                security_group: row.get(1)?,
                protocol: row.get(2)?,
                host_port: row.get(3)?,
                target_port: row.get(4)?,
                source_cidr: row.get(5)?,
                target_vm: row.get(6)?,
                enable_dnat: row.get::<_, i32>(7)? != 0,
            })
        })?;
        rows.collect()
    }

    pub fn attach_security_group_to_vm(
        &self,
        security_group: &str,
        vm_id: &str,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR IGNORE INTO security_group_vm_attachments (security_group, vm_id)
             VALUES (?1, ?2)",
            params![security_group, vm_id],
        )?;
        Ok(())
    }

    pub fn detach_security_group_from_vm(
        &self,
        security_group: &str,
        vm_id: &str,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "DELETE FROM security_group_vm_attachments WHERE security_group = ?1 AND vm_id = ?2",
            params![security_group, vm_id],
        )?;
        Ok(rows > 0)
    }

    pub fn list_security_group_vm_attachments(
        &self,
        security_group: &str,
    ) -> Result<Vec<SecurityGroupVmAttachmentRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT security_group, vm_id
             FROM security_group_vm_attachments
             WHERE security_group = ?1
             ORDER BY vm_id ASC",
        )?;
        let rows = stmt.query_map(params![security_group], |row| {
            Ok(SecurityGroupVmAttachmentRow {
                security_group: row.get(0)?,
                vm_id: row.get(1)?,
            })
        })?;
        rows.collect()
    }

    pub fn list_security_groups_for_vm(&self, vm_id: &str) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT security_group FROM security_group_vm_attachments WHERE vm_id = ?1 ORDER BY security_group ASC",
        )?;
        let rows = stmt.query_map(params![vm_id], |row| row.get(0))?;
        rows.collect()
    }

    pub fn attach_security_group_to_network(
        &self,
        security_group: &str,
        network_name: &str,
        node_id: &str,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR IGNORE INTO security_group_network_attachments (security_group, network_name, node_id)
             VALUES (?1, ?2, ?3)",
            params![security_group, network_name, node_id],
        )?;
        Ok(())
    }

    pub fn detach_security_group_from_network(
        &self,
        security_group: &str,
        network_name: &str,
        node_id: &str,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "DELETE FROM security_group_network_attachments
             WHERE security_group = ?1 AND network_name = ?2 AND node_id = ?3",
            params![security_group, network_name, node_id],
        )?;
        Ok(rows > 0)
    }

    pub fn list_security_group_network_attachments(
        &self,
        security_group: &str,
    ) -> Result<Vec<SecurityGroupNetworkAttachmentRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT security_group, network_name, node_id
             FROM security_group_network_attachments
             WHERE security_group = ?1
             ORDER BY node_id ASC, network_name ASC",
        )?;
        let rows = stmt.query_map(params![security_group], |row| {
            Ok(SecurityGroupNetworkAttachmentRow {
                security_group: row.get(0)?,
                network_name: row.get(1)?,
                node_id: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn list_security_groups_for_network(
        &self,
        network_name: &str,
        node_id: &str,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT security_group FROM security_group_network_attachments
             WHERE network_name = ?1 AND node_id = ?2
             ORDER BY security_group ASC",
        )?;
        let rows = stmt.query_map(params![network_name, node_id], |row| row.get(0))?;
        rows.collect()
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

    /// Compare-and-swap for `auto_start`: set to `new_value` only if the row
    /// currently equals `expected_current`. Used to roll back a failed node
    /// push without clobbering a concurrent successful update.
    pub fn set_vm_auto_start_if_current(
        &self,
        vm_id_or_name: &str,
        expected_current: bool,
        new_value: bool,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "UPDATE vms SET auto_start = ?1 WHERE (id = ?2 OR name = ?2) AND auto_start = ?3",
            params![new_value as i32, vm_id_or_name, expected_current as i32,],
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

    pub fn upsert_ssh_key(&self, name: &str, public_key: &str) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO ssh_keys (name, public_key) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET public_key = excluded.public_key",
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
        conn.execute("DELETE FROM vm_ssh_keys WHERE key_name = ?1", params![name])?;
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

    pub fn get_vm_ssh_key_names(&self, vm_id: &str) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare("SELECT key_name FROM vm_ssh_keys WHERE vm_id = ?1")?;
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
        dc_id: row.get(15)?,
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

fn row_to_workload(row: &rusqlite::Row) -> Result<WorkloadRow, rusqlite::Error> {
    Ok(WorkloadRow {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        node_id: row.get(3)?,
        runtime_state: row.get(4)?,
        desired_state: row.get(5)?,
        vm_id: row.get(6)?,
        container_image: row.get(7)?,
        network: row.get(8)?,
        storage_backend: row.get(9)?,
        storage_size_bytes: row.get(10)?,
        created_at: row.get(11)?,
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
            dc_id: "DC1".to_string(),
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

    fn test_workload(node_id: &str) -> WorkloadRow {
        WorkloadRow {
            id: "wl-1".to_string(),
            name: "workload-1".to_string(),
            kind: "container".to_string(),
            node_id: node_id.to_string(),
            runtime_state: "running".to_string(),
            desired_state: "running".to_string(),
            vm_id: String::new(),
            container_image: "nginx:alpine".to_string(),
            network: "default".to_string(),
            storage_backend: "filesystem".to_string(),
            storage_size_bytes: 1024,
            created_at: String::new(),
        }
    }

    #[test]
    fn open_rejects_path_with_dot_dot_segments() {
        for bad in [
            "../evil.db",
            "foo/../../etc/passwd",
            "/tmp/myapp/../../../../var/tmp/malicious_dir/db.sqlite",
            r"foo\..\..\secret.db",
            "file:../../../tmp/x.db",
        ] {
            let err = Database::open(bad).expect_err("path traversal should be rejected");
            let msg = format!("{err:#}");
            assert!(
                msg.contains("..") || msg.contains("parent directory"),
                "unexpected error for {bad:?}: {msg}"
            );
        }
    }

    #[test]
    fn open_creates_parent_for_safe_path_under_temp() {
        let unique = uuid::Uuid::new_v4();
        let root = std::env::temp_dir().join(format!("kcore-db-open-test-{unique}"));
        let path = root.join("nested/controller.sqlite");
        let path_str = path.to_str().expect("utf-8 temp path");
        let _ = std::fs::remove_dir_all(&root);

        let db = Database::open(path_str).expect("open db with mkdir");
        drop(db);

        assert!(path.is_file(), "database file should exist at {path_str}");
        let _ = std::fs::remove_dir_all(&root);
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
    fn set_vm_auto_start_if_current_skips_when_value_mismatch() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");
        db.insert_vm(&test_vm(&node.id)).expect("insert vm");

        assert!(
            db.set_vm_auto_start_if_current("web-1", true, false)
                .expect("cas"),
            "expected update from true to false"
        );
        assert!(
            !db.set_vm_auto_start_if_current("web-1", true, true)
                .expect("cas skip"),
            "expected no update when expected_current does not match"
        );
        let v = db.get_vm("vm-1").expect("get vm").expect("vm");
        assert!(!v.auto_start);
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
    fn heartbeat_updates_timestamp_but_preserves_status_for_non_approved_nodes() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.approval_status = "pending".to_string();
        node.status = "pending".to_string();
        db.upsert_node(&node).expect("insert");

        let updated = db
            .update_heartbeat(&node.id, 1, 1000, -1, "")
            .expect("heartbeat");
        assert!(
            updated,
            "heartbeat should update a registered node regardless of approval"
        );

        let got = db.get_node(&node.id).expect("get").expect("exists");
        assert_eq!(
            got.status, "pending",
            "status should still be pending for non-approved"
        );
        assert!(
            !got.last_heartbeat.is_empty(),
            "heartbeat timestamp should be set"
        );
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
    fn allocate_vm_ip_global_is_unique_across_nodes() {
        let db = Database::open(":memory:").expect("open db");
        let mut n1 = test_node();
        n1.id = "n1".to_string();
        let mut n2 = test_node();
        n2.id = "n2".to_string();
        db.upsert_node(&n1).expect("insert n1");
        db.upsert_node(&n2).expect("insert n2");

        for nid in ["n1", "n2"] {
            db.insert_network(&NetworkRow {
                name: "overlay".to_string(),
                external_ip: "0.0.0.0".to_string(),
                gateway_ip: "10.50.0.1".to_string(),
                internal_netmask: "255.255.255.0".to_string(),
                node_id: nid.to_string(),
                allowed_tcp_ports: String::new(),
                allowed_udp_ports: String::new(),
                vlan_id: 0,
                network_type: "vxlan".to_string(),
                enable_outbound_nat: false,
                vni: 10200,
                next_ip: 2,
            })
            .expect("insert");
        }

        let ip1 = db.allocate_vm_ip_global("overlay").expect("alloc 1");
        assert_eq!(ip1, "10.50.0.2");

        let ip2 = db.allocate_vm_ip_global("overlay").expect("alloc 2");
        assert_eq!(ip2, "10.50.0.3");

        let ip3 = db.allocate_vm_ip_global("overlay").expect("alloc 3");
        assert_eq!(ip3, "10.50.0.4");

        let nets = db.list_networks_by_name("overlay").expect("list");
        for n in &nets {
            assert_eq!(n.next_ip, 5, "both node rows should be at 5");
        }
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
    fn workload_roundtrip_works() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");

        let mut wl = test_workload(&node.id);
        db.upsert_workload(&wl).expect("upsert workload");

        let got = db
            .get_workload(&wl.id)
            .expect("get workload")
            .expect("exists");
        assert_eq!(got.kind, "container");
        assert_eq!(got.container_image, "nginx:alpine");

        let listed = db
            .list_workloads(Some("container"), Some(&node.id))
            .expect("list workloads");
        assert_eq!(listed.len(), 1);

        assert!(db
            .update_workload_runtime_state(&wl.id, "stopped")
            .expect("update state"));
        assert!(db
            .update_workload_desired_state(&wl.id, "stopped")
            .expect("update desired"));
        wl = db.get_workload(&wl.id).expect("reload").expect("exists");
        assert_eq!(wl.runtime_state, "stopped");
        assert_eq!(wl.desired_state, "stopped");

        assert!(db
            .delete_workload_by_id_or_name(&wl.id)
            .expect("delete workload"));
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

        db.upsert_replication_ack("peer-a", 7)
            .expect("upsert should remain monotonic");
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
            .insert_compensation_job(
                conflict_id,
                "vm/v1",
                "op-loser",
                "vm.update",
                r#"{"vmId":"vm/v1","cpu":4}"#,
            )
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

    #[test]
    fn security_group_roundtrip_with_rules() {
        let db = Database::open(":memory:").expect("db");
        db.upsert_security_group(&SecurityGroupRow {
            name: "web".to_string(),
            description: "web ingress".to_string(),
            created_at: String::new(),
        })
        .expect("insert sg");
        db.replace_security_group_rules(
            "web",
            &[SecurityGroupRuleRow {
                id: "r1".to_string(),
                security_group: "web".to_string(),
                protocol: "tcp".to_string(),
                host_port: 443,
                target_port: 8443,
                source_cidr: "0.0.0.0/0".to_string(),
                target_vm: "vm-1".to_string(),
                enable_dnat: true,
            }],
        )
        .expect("insert rule");

        let got = db.get_security_group("web").expect("read").expect("exists");
        assert_eq!(got.name, "web");
        assert_eq!(got.description, "web ingress");
        let rules = db.list_security_group_rules("web").expect("rules");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].protocol, "tcp");
        assert_eq!(rules[0].host_port, 443);
        assert!(rules[0].enable_dnat);
    }

    #[test]
    fn security_group_attachments_roundtrip() {
        let db = Database::open(":memory:").expect("db");
        let node = NodeRow {
            id: "node-1".to_string(),
            hostname: "node-1".to_string(),
            address: "10.0.0.1:9091".to_string(),
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
            dc_id: "DC1".to_string(),
        };
        db.upsert_node(&node).expect("node");
        db.insert_vm(&VmRow {
            id: "vm-1".to_string(),
            name: "vm-1".to_string(),
            cpu: 2,
            memory_bytes: 1024,
            image_path: "/tmp/img.raw".to_string(),
            image_url: String::new(),
            image_sha256: String::new(),
            image_format: "raw".to_string(),
            image_size: 1024,
            network: "private".to_string(),
            auto_start: true,
            node_id: "node-1".to_string(),
            created_at: String::new(),
            runtime_state: "running".to_string(),
            cloud_init_user_data: String::new(),
            storage_backend: "filesystem".to_string(),
            storage_size_bytes: 1024 * 1024,
            vm_ip: "10.240.0.22".to_string(),
        })
        .expect("vm");
        db.insert_network(&NetworkRow {
            name: "private".to_string(),
            external_ip: "203.0.113.10".to_string(),
            gateway_ip: "10.240.0.1".to_string(),
            internal_netmask: "255.255.255.0".to_string(),
            node_id: "node-1".to_string(),
            allowed_tcp_ports: String::new(),
            allowed_udp_ports: String::new(),
            vlan_id: 0,
            network_type: "nat".to_string(),
            enable_outbound_nat: true,
            vni: 0,
            next_ip: 2,
        })
        .expect("network");
        db.upsert_security_group(&SecurityGroupRow {
            name: "web".to_string(),
            description: String::new(),
            created_at: String::new(),
        })
        .expect("sg");
        db.attach_security_group_to_vm("web", "vm-1")
            .expect("attach vm");
        db.attach_security_group_to_network("web", "private", "node-1")
            .expect("attach network");

        let vm_groups = db.list_security_groups_for_vm("vm-1").expect("vm groups");
        let net_groups = db
            .list_security_groups_for_network("private", "node-1")
            .expect("net groups");
        assert_eq!(vm_groups, vec!["web".to_string()]);
        assert_eq!(net_groups, vec!["web".to_string()]);
    }

    #[test]
    fn upsert_peer_address_only_preserves_dc_id() {
        let db = Database::open(":memory:").expect("open db");
        db.upsert_controller_peer("ctrl-dc2", "10.0.1.50:9090", "DC2")
            .expect("full upsert");
        let peers = db.list_controller_peers().expect("list");
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].dc_id, "DC2");

        db.upsert_controller_peer_address_only("ctrl-dc2", "10.0.1.50:9091")
            .expect("address-only upsert");
        let peers = db.list_controller_peers().expect("list");
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].dc_id, "DC2");
        assert_eq!(peers[0].address, "10.0.1.50:9091");
    }

    #[test]
    fn upsert_peer_address_only_inserts_with_default_dc() {
        let db = Database::open(":memory:").expect("open db");
        db.upsert_controller_peer_address_only("ctrl-new", "10.0.2.1:9090")
            .expect("address-only insert");
        let peers = db.list_controller_peers().expect("list");
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].dc_id, "DC1");
        assert_eq!(peers[0].address, "10.0.2.1:9090");
    }
}

/// Property-based tests (Phase 3) — database CRUD invariants.
///
/// `db.rs` is the source of truth for the entire system: a subtle bug
/// here (a VM that survives its node, an upsert that creates a duplicate,
/// a heartbeat that flips approval state) propagates silently and can
/// cause wrong Nix configs to be pushed. The example tests in
/// `mod tests` cover one concrete row at a time; these proptests cover
/// the same invariants over thousands of randomised rows.
///
/// Each test opens a fresh `:memory:` SQLite database so cases are
/// independent. The `Database::open(":memory:")` call is cheap (~µs)
/// because the in-memory file lives only for the duration of the test.
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Build a `NodeRow` with caller-supplied identifying / quantitative
    /// fields and sensible defaults for the rest. Centralised so each
    /// test only has to randomise the fields it cares about.
    fn make_node(
        id: &str,
        hostname: &str,
        address: &str,
        cpu_cores: i32,
        memory_bytes: i64,
        cpu_used: i32,
        memory_used: i64,
        disable_vxlan: bool,
        cert_expiry_days: i32,
        dc_id: &str,
    ) -> NodeRow {
        NodeRow {
            id: id.to_string(),
            hostname: hostname.to_string(),
            address: address.to_string(),
            cpu_cores,
            memory_bytes,
            status: "ready".to_string(),
            last_heartbeat: String::new(),
            gateway_interface: "eno1".to_string(),
            cpu_used,
            memory_used,
            storage_backend: "filesystem".to_string(),
            disable_vxlan,
            approval_status: "approved".to_string(),
            cert_expiry_days,
            luks_method: String::new(),
            dc_id: dc_id.to_string(),
        }
    }

    /// Build a minimal `VmRow` for a given node, randomising only the
    /// fields each test cares about.
    #[allow(clippy::too_many_arguments)]
    fn make_vm(
        id: &str,
        name: &str,
        node_id: &str,
        cpu: i32,
        memory_bytes: i64,
        auto_start: bool,
    ) -> VmRow {
        VmRow {
            id: id.to_string(),
            name: name.to_string(),
            cpu,
            memory_bytes,
            image_path: format!("/var/lib/kcore/images/{name}.raw"),
            image_url: String::new(),
            image_sha256: String::new(),
            image_format: "raw".to_string(),
            image_size: 8192,
            network: "default".to_string(),
            auto_start,
            node_id: node_id.to_string(),
            created_at: String::new(),
            runtime_state: "unknown".to_string(),
            cloud_init_user_data: String::new(),
            storage_backend: "filesystem".to_string(),
            storage_size_bytes: 0,
            vm_ip: String::new(),
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            // SQLite open + a handful of statements per case is fast
            // enough for ~256 cases per test; 2 000 (the controller-
            // pure proptest budget) would add noticeable wall time
            // without finding new bugs.
            cases: 256,
            .. ProptestConfig::default()
        })]

        /// **Node CRUD round-trip**: for any randomised `NodeRow`,
        /// `upsert_node` followed by `get_node` returns a row whose
        /// scalar fields equal the inserted ones.
        #[test]
        fn node_upsert_then_get_returns_same_row(
            id in "[a-z0-9-]{1,12}",
            hostname in "[a-z0-9-]{1,12}",
            address in "[a-z0-9.:-]{1,24}",
            cpu_cores in 1i32..=128,
            memory_bytes in 1i64..(1i64 << 40),
            cpu_used in 0i32..=128,
            memory_used in 0i64..(1i64 << 40),
            disable_vxlan in any::<bool>(),
            cert_expiry_days in -1i32..=3650,
            dc_id in prop::sample::select(vec!["DC1", "DC2", "EU-W"]),
        ) {
            let db = Database::open(":memory:").expect("open db");
            let node = make_node(
                &id, &hostname, &address,
                cpu_cores, memory_bytes,
                cpu_used, memory_used,
                disable_vxlan, cert_expiry_days,
                dc_id,
            );
            db.upsert_node(&node).unwrap();

            let got = db.get_node(&id).unwrap().expect("node exists after upsert");
            prop_assert_eq!(&got.id, &node.id);
            prop_assert_eq!(&got.hostname, &node.hostname);
            prop_assert_eq!(&got.address, &node.address);
            prop_assert_eq!(got.cpu_cores, node.cpu_cores);
            prop_assert_eq!(got.memory_bytes, node.memory_bytes);
            prop_assert_eq!(got.cpu_used, node.cpu_used);
            prop_assert_eq!(got.memory_used, node.memory_used);
            prop_assert_eq!(got.disable_vxlan, node.disable_vxlan);
            prop_assert_eq!(got.cert_expiry_days, node.cert_expiry_days);
            prop_assert_eq!(&got.dc_id, &node.dc_id);
        }

        /// **Upsert idempotence**: upserting the same node twice yields
        /// a single row in `list_nodes` (no PK duplicate, no orphan).
        #[test]
        fn node_upsert_is_idempotent(
            id in "[a-z0-9-]{1,12}",
            cpu_cores in 1i32..=128,
            memory_bytes in 1i64..(1i64 << 40),
        ) {
            let db = Database::open(":memory:").expect("open db");
            let node = make_node(
                &id, "h", "127.0.0.1:9091",
                cpu_cores, memory_bytes,
                0, 0, false, 0, "DC1",
            );
            db.upsert_node(&node).unwrap();
            db.upsert_node(&node).unwrap();

            let nodes = db.list_nodes().unwrap();
            prop_assert_eq!(nodes.len(), 1);
            prop_assert_eq!(&nodes[0].id, &id);
            prop_assert_eq!(nodes[0].cpu_cores, cpu_cores);
            prop_assert_eq!(nodes[0].memory_bytes, memory_bytes);
        }

        /// **Upsert updates fields in place**: upserting a node twice
        /// with different `address` values yields exactly one row
        /// whose address is the most recently provided one.
        #[test]
        fn node_upsert_updates_address_in_place(
            id in "[a-z0-9-]{1,12}",
            addr_a in "[a-z0-9.:-]{1,24}",
            addr_b in "[a-z0-9.:-]{1,24}",
        ) {
            prop_assume!(addr_a != addr_b);
            let db = Database::open(":memory:").expect("open db");
            let mut node = make_node(
                &id, "h", &addr_a, 1, 1024, 0, 0, false, 0, "DC1",
            );
            db.upsert_node(&node).unwrap();
            node.address = addr_b.clone();
            db.upsert_node(&node).unwrap();

            let nodes = db.list_nodes().unwrap();
            prop_assert_eq!(nodes.len(), 1);
            prop_assert_eq!(&nodes[0].address, &addr_b);
        }

        /// **Foreign-key integrity on VMs**: inserting a `VmRow` whose
        /// `node_id` does not exist in `nodes` MUST fail. SQLite's
        /// `PRAGMA foreign_keys=ON` is enabled in `Database::open`, so
        /// this is the property that protects the controller from
        /// scheduling a workload onto a phantom node.
        #[test]
        fn vm_insert_rejects_unknown_node_id(
            present_node in "[a-z0-9-]{1,8}",
            missing_node in "[a-z0-9-]{1,8}",
            vm_id in "[a-z0-9-]{1,8}",
        ) {
            prop_assume!(present_node != missing_node);
            let db = Database::open(":memory:").expect("open db");
            db.upsert_node(&make_node(
                &present_node, "h", "127.0.0.1:9091",
                1, 1024, 0, 0, false, 0, "DC1",
            )).unwrap();

            let vm = make_vm(&vm_id, &vm_id, &missing_node, 1, 1024, false);
            let err = db.insert_vm(&vm).unwrap_err();
            // Structured match: rusqlite surfaces FK violations as
            // SqliteFailure with code = ConstraintViolation and
            // extended_code = SQLITE_CONSTRAINT_FOREIGNKEY (787).
            // The integer literal is used because rusqlite does not
            // re-export the extended-code constant publicly.
            const SQLITE_CONSTRAINT_FOREIGNKEY: i32 = 787;
            match &err {
                rusqlite::Error::SqliteFailure(sqlite_err, _)
                    if sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation
                        && sqlite_err.extended_code == SQLITE_CONSTRAINT_FOREIGNKEY => {}
                other => prop_assert!(
                    false,
                    "expected SQLITE_CONSTRAINT_FOREIGNKEY for unknown node, got: {other}"
                ),
            }
        }

        /// **VM CRUD round-trip**: insert + get_vm returns a row whose
        /// scalar fields match the inserted ones.
        #[test]
        fn vm_insert_then_get_returns_same_row(
            node_id in "[a-z0-9-]{1,8}",
            vm_id in "[a-z0-9-]{1,8}",
            name in "[a-z0-9-]{1,12}",
            cpu in 1i32..=64,
            memory_bytes in 1i64..(1i64 << 40),
            auto_start in any::<bool>(),
        ) {
            let db = Database::open(":memory:").expect("open db");
            db.upsert_node(&make_node(
                &node_id, "h", "127.0.0.1:9091",
                4, 1024, 0, 0, false, 0, "DC1",
            )).unwrap();
            let vm = make_vm(&vm_id, &name, &node_id, cpu, memory_bytes, auto_start);
            db.insert_vm(&vm).unwrap();

            let got = db.get_vm(&vm_id).unwrap().expect("vm exists");
            prop_assert_eq!(&got.id, &vm.id);
            prop_assert_eq!(&got.name, &vm.name);
            prop_assert_eq!(got.cpu, vm.cpu);
            prop_assert_eq!(got.memory_bytes, vm.memory_bytes);
            prop_assert_eq!(got.auto_start, vm.auto_start);
            prop_assert_eq!(&got.node_id, &vm.node_id);
        }

        /// **Delete consistency**: after `delete_vm_by_id_or_name`, the
        /// VM disappears from every observable view.
        #[test]
        fn vm_delete_removes_from_all_views(
            node_id in "[a-z0-9-]{1,8}",
            vm_id in "[a-z0-9-]{1,8}",
        ) {
            let db = Database::open(":memory:").expect("open db");
            db.upsert_node(&make_node(
                &node_id, "h", "127.0.0.1:9091",
                4, 1024, 0, 0, false, 0, "DC1",
            )).unwrap();
            // Use the same string for id and name so deletion by either
            // matches the row.
            let vm = make_vm(&vm_id, &vm_id, &node_id, 1, 1024, false);
            db.insert_vm(&vm).unwrap();
            prop_assert!(db.get_vm(&vm_id).unwrap().is_some());

            let deleted = db.delete_vm_by_id_or_name(&vm_id).unwrap();
            prop_assert!(deleted);
            prop_assert!(db.get_vm(&vm_id).unwrap().is_none());
            prop_assert!(db.find_node_for_vm(&vm_id).unwrap().is_none());
            let listed = db.list_vms().unwrap();
            prop_assert!(
                !listed.iter().any(|v| v.id == vm_id),
                "deleted vm {vm_id:?} still appears in list_vms()"
            );
        }

        /// **Heartbeat idempotence (modulo timestamp)**: calling
        /// `update_heartbeat` twice with identical arguments produces
        /// the same node state in every field except `last_heartbeat`
        /// (which is `datetime('now')` and may shift between calls).
        ///
        /// This is the invariant that protects the scheduler from
        /// flapping resource reservations on a quiet, healthy node.
        #[test]
        fn heartbeat_is_idempotent_modulo_timestamp(
            node_id in "[a-z0-9-]{1,8}",
            cpu_used in 0i32..=64,
            mem_used in 0i64..(1i64 << 36),
            cert_days in -1i32..=3650,
            luks in prop::sample::select(vec!["", "tpm", "passphrase"]),
        ) {
            let db = Database::open(":memory:").expect("open db");
            db.upsert_node(&make_node(
                &node_id, "h", "127.0.0.1:9091",
                4, 1024, 0, 0, false, 0, "DC1",
            )).unwrap();

            db.update_heartbeat(&node_id, cpu_used, mem_used, cert_days, luks).unwrap();
            let after_first = db.get_node(&node_id).unwrap().expect("node");
            db.update_heartbeat(&node_id, cpu_used, mem_used, cert_days, luks).unwrap();
            let after_second = db.get_node(&node_id).unwrap().expect("node");

            prop_assert_eq!(after_first.cpu_used, after_second.cpu_used);
            prop_assert_eq!(after_first.memory_used, after_second.memory_used);
            prop_assert_eq!(after_first.cert_expiry_days, after_second.cert_expiry_days);
            prop_assert_eq!(&after_first.luks_method, &after_second.luks_method);
            prop_assert_eq!(&after_first.status, &after_second.status);
            prop_assert_eq!(&after_first.approval_status, &after_second.approval_status);
        }

        /// **DiskLayout CRUD round-trip**: for any randomised
        /// `DiskLayoutRow` whose `node_id` matches an existing node,
        /// `upsert_disk_layout` followed by `get_disk_layout` returns
        /// a row whose user-set fields equal the inserted ones.
        #[test]
        fn disk_layout_upsert_then_get_returns_same_row(
            node_id in "[a-z0-9-]{1,8}",
            name in "[a-z0-9-]{1,12}",
            generation in 1i64..=1024,
            layout_nix in "[ -~\n]{0,256}",
        ) {
            let db = Database::open(":memory:").expect("open db");
            db.upsert_node(&make_node(
                &node_id, "h", "127.0.0.1:9091",
                4, 1024, 0, 0, false, 0, "DC1",
            )).unwrap();

            let layout = DiskLayoutRow {
                name: name.clone(),
                node_id: node_id.clone(),
                generation,
                layout_nix: layout_nix.clone(),
                created_at: String::new(),
                updated_at: String::new(),
            };
            db.upsert_disk_layout(&layout).unwrap();

            let got = db.get_disk_layout(&name).unwrap().expect("layout exists");
            prop_assert_eq!(&got.name, &name);
            prop_assert_eq!(&got.node_id, &node_id);
            prop_assert_eq!(got.generation, generation);
            prop_assert_eq!(&got.layout_nix, &layout_nix);
            prop_assert!(!got.created_at.is_empty());
            prop_assert!(!got.updated_at.is_empty());
        }

        /// **DiskLayout upsert is idempotent**: applying the same row
        /// twice yields exactly one row in `list_disk_layouts`.
        #[test]
        fn disk_layout_upsert_is_idempotent(
            node_id in "[a-z0-9-]{1,8}",
            name in "[a-z0-9-]{1,12}",
        ) {
            let db = Database::open(":memory:").expect("open db");
            db.upsert_node(&make_node(
                &node_id, "h", "127.0.0.1:9091",
                4, 1024, 0, 0, false, 0, "DC1",
            )).unwrap();

            let layout = DiskLayoutRow {
                name: name.clone(),
                node_id: node_id.clone(),
                generation: 1,
                layout_nix: "{ disko.devices = {}; }".to_string(),
                created_at: String::new(),
                updated_at: String::new(),
            };
            db.upsert_disk_layout(&layout).unwrap();
            db.upsert_disk_layout(&layout).unwrap();

            let listed = db.list_disk_layouts(None).unwrap();
            prop_assert_eq!(listed.len(), 1);
            prop_assert_eq!(&listed[0].name, &name);

            let only_node = db.list_disk_layouts(Some(&node_id)).unwrap();
            prop_assert_eq!(only_node.len(), 1);
        }

        /// **DiskLayout FK integrity**: inserting a layout whose
        /// `node_id` does not exist must fail with a SQLite FK
        /// constraint violation.
        #[test]
        fn disk_layout_rejects_unknown_node_id(
            present_node in "[a-z0-9-]{1,8}",
            missing_node in "[a-z0-9-]{1,8}",
            name in "[a-z0-9-]{1,12}",
        ) {
            prop_assume!(present_node != missing_node);
            let db = Database::open(":memory:").expect("open db");
            db.upsert_node(&make_node(
                &present_node, "h", "127.0.0.1:9091",
                1, 1024, 0, 0, false, 0, "DC1",
            )).unwrap();

            let layout = DiskLayoutRow {
                name,
                node_id: missing_node,
                generation: 1,
                layout_nix: "{}".to_string(),
                created_at: String::new(),
                updated_at: String::new(),
            };
            const SQLITE_CONSTRAINT_FOREIGNKEY: i32 = 787;
            match db.upsert_disk_layout(&layout) {
                Ok(_) => prop_assert!(false, "expected FK violation, got Ok"),
                Err(rusqlite::Error::SqliteFailure(sqlite_err, _))
                    if sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation
                        && sqlite_err.extended_code == SQLITE_CONSTRAINT_FOREIGNKEY => {}
                Err(other) => prop_assert!(
                    false,
                    "expected SQLITE_CONSTRAINT_FOREIGNKEY, got: {other}"
                ),
            }
        }

        /// **DiskLayout delete removes from every view**: after
        /// `delete_disk_layout`, the layout disappears from
        /// `get_disk_layout` and `list_disk_layouts` and any status
        /// row is cascaded away.
        #[test]
        fn disk_layout_delete_removes_from_all_views(
            node_id in "[a-z0-9-]{1,8}",
            name in "[a-z0-9-]{1,12}",
        ) {
            let db = Database::open(":memory:").expect("open db");
            db.upsert_node(&make_node(
                &node_id, "h", "127.0.0.1:9091",
                4, 1024, 0, 0, false, 0, "DC1",
            )).unwrap();

            let layout = DiskLayoutRow {
                name: name.clone(),
                node_id: node_id.clone(),
                generation: 7,
                layout_nix: "{}".to_string(),
                created_at: String::new(),
                updated_at: String::new(),
            };
            db.upsert_disk_layout(&layout).unwrap();
            db.upsert_disk_layout_status(&DiskLayoutStatusRow {
                name: name.clone(),
                observed_generation: 7,
                phase: "applied".to_string(),
                refusal_reason: String::new(),
                message: "ok".to_string(),
                last_transition_at: String::new(),
            }).unwrap();

            let deleted = db.delete_disk_layout(&name).unwrap();
            prop_assert!(deleted);
            prop_assert!(db.get_disk_layout(&name).unwrap().is_none());
            prop_assert!(db.get_disk_layout_status(&name).unwrap().is_none());
            let listed = db.list_disk_layouts(None).unwrap();
            prop_assert!(!listed.iter().any(|l| l.name == name));
        }

        /// **Reconciler queue invariant**: a layout shows up in
        /// `list_disk_layouts_needing_reconcile` iff it has no status
        /// row, or its status `observed_generation` is strictly
        /// behind the layout's `generation`. Once status catches up,
        /// it must drop out of the queue.
        #[test]
        fn disk_layout_reconcile_queue_tracks_observed_generation(
            node_id in "[a-z0-9-]{1,8}",
            name in "[a-z0-9-]{1,12}",
            generation in 1i64..=64,
        ) {
            let db = Database::open(":memory:").expect("open db");
            db.upsert_node(&make_node(
                &node_id, "h", "127.0.0.1:9091",
                4, 1024, 0, 0, false, 0, "DC1",
            )).unwrap();

            db.upsert_disk_layout(&DiskLayoutRow {
                name: name.clone(),
                node_id: node_id.clone(),
                generation,
                layout_nix: "{}".to_string(),
                created_at: String::new(),
                updated_at: String::new(),
            }).unwrap();

            let pending = db.list_disk_layouts_needing_reconcile().unwrap();
            prop_assert!(pending.iter().any(|l| l.name == name),
                "freshly created layout must be queued for reconcile");

            db.upsert_disk_layout_status(&DiskLayoutStatusRow {
                name: name.clone(),
                observed_generation: generation,
                phase: "applied".to_string(),
                refusal_reason: String::new(),
                message: String::new(),
                last_transition_at: String::new(),
            }).unwrap();
            let after = db.list_disk_layouts_needing_reconcile().unwrap();
            prop_assert!(!after.iter().any(|l| l.name == name),
                "layout with observed_generation == generation must NOT be queued");

            db.upsert_disk_layout(&DiskLayoutRow {
                name: name.clone(),
                node_id: node_id.clone(),
                generation: generation + 1,
                layout_nix: "{ updated = true; }".to_string(),
                created_at: String::new(),
                updated_at: String::new(),
            }).unwrap();
            let bumped = db.list_disk_layouts_needing_reconcile().unwrap();
            prop_assert!(bumped.iter().any(|l| l.name == name),
                "layout whose generation moved past observed_generation must be re-queued");
        }
    }
}
