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
}

#[derive(Debug, Clone)]
pub struct NetworkRow {
    pub name: String,
    pub external_ip: String,
    pub gateway_ip: String,
    pub internal_netmask: String,
    pub node_id: String,
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
            "CREATE TABLE IF NOT EXISTS nodes (
                id TEXT PRIMARY KEY,
                hostname TEXT NOT NULL,
                address TEXT NOT NULL,
                cpu_cores INTEGER NOT NULL DEFAULT 0,
                memory_bytes INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'unknown',
                last_heartbeat TEXT NOT NULL DEFAULT '',
                gateway_interface TEXT NOT NULL DEFAULT ''
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
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS networks (
                name TEXT NOT NULL,
                external_ip TEXT NOT NULL,
                gateway_ip TEXT NOT NULL,
                internal_netmask TEXT NOT NULL DEFAULT '255.255.255.0',
                node_id TEXT NOT NULL REFERENCES nodes(id),
                PRIMARY KEY (name, node_id)
            );",
        )?;
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
        Ok(())
    }

    pub fn upsert_node(&self, node: &NodeRow) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO nodes (id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                hostname=excluded.hostname,
                address=excluded.address,
                cpu_cores=excluded.cpu_cores,
                memory_bytes=excluded.memory_bytes,
                status=excluded.status,
                last_heartbeat=excluded.last_heartbeat,
                gateway_interface=excluded.gateway_interface",
            params![
                node.id,
                node.hostname,
                node.address,
                node.cpu_cores,
                node.memory_bytes,
                node.status,
                node.last_heartbeat,
                node.gateway_interface,
            ],
        )?;
        Ok(())
    }

    pub fn update_heartbeat(
        &self,
        node_id: &str,
        cpu_used: i32,
        mem_used: i64,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute(
            "UPDATE nodes SET last_heartbeat = datetime('now'), status = 'ready' WHERE id = ?1",
            params![node_id],
        )?;
        drop(conn);
        if rows > 0 && (cpu_used > 0 || mem_used > 0) {
            // usage fields could be stored if needed
        }
        Ok(rows > 0)
    }

    pub fn get_node(&self, node_id: &str) -> Result<Option<NodeRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface FROM nodes WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![node_id], row_to_node)?;
        rows.next().transpose()
    }

    pub fn list_nodes(&self) -> Result<Vec<NodeRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface FROM nodes",
        )?;
        let rows = stmt.query_map([], row_to_node)?;
        rows.collect()
    }

    pub fn get_node_by_address(&self, address: &str) -> Result<Option<NodeRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface FROM nodes WHERE address = ?1",
        )?;
        let mut rows = stmt.query_map(params![address], row_to_node)?;
        rows.next().transpose()
    }

    pub fn insert_vm(&self, vm: &VmRow) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO vms (id, name, cpu, memory_bytes, image_path, image_url, image_sha256, image_format, image_size, network, auto_start, node_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, datetime('now'))",
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
            ],
        )?;
        Ok(())
    }

    pub fn delete_vm(&self, vm_id: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let rows = conn.execute("DELETE FROM vms WHERE id = ?1", params![vm_id])?;
        Ok(rows > 0)
    }

    pub fn get_vm(&self, vm_id: &str) -> Result<Option<VmRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, cpu, memory_bytes, image_path, image_url, image_sha256, image_format, image_size, network, auto_start, node_id, created_at FROM vms WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![vm_id], row_to_vm)?;
        rows.next().transpose()
    }

    pub fn list_vms(&self) -> Result<Vec<VmRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, cpu, memory_bytes, image_path, image_url, image_sha256, image_format, image_size, network, auto_start, node_id, created_at FROM vms",
        )?;
        let rows = stmt.query_map([], row_to_vm)?;
        rows.collect()
    }

    pub fn list_vms_for_node(&self, node_id: &str) -> Result<Vec<VmRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, cpu, memory_bytes, image_path, image_url, image_sha256, image_format, image_size, network, auto_start, node_id, created_at FROM vms WHERE node_id = ?1",
        )?;
        let rows = stmt.query_map(params![node_id], row_to_vm)?;
        rows.collect()
    }

    pub fn insert_network(&self, network: &NetworkRow) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO networks (name, external_ip, gateway_ip, internal_netmask, node_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                network.name,
                network.external_ip,
                network.gateway_ip,
                network.internal_netmask,
                network.node_id
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
            "SELECT name, external_ip, gateway_ip, internal_netmask, node_id
             FROM networks
             WHERE node_id = ?1 AND name = ?2",
        )?;
        let mut rows = stmt.query_map(params![node_id, name], row_to_network)?;
        rows.next().transpose()
    }

    pub fn list_networks(&self) -> Result<Vec<NetworkRow>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT name, external_ip, gateway_ip, internal_netmask, node_id
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
            "SELECT name, external_ip, gateway_ip, internal_netmask, node_id
             FROM networks
             WHERE node_id = ?1",
        )?;
        let rows = stmt.query_map(params![node_id], row_to_network)?;
        rows.collect()
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
}

fn row_to_node(row: &rusqlite::Row) -> Result<NodeRow, rusqlite::Error> {
    Ok(NodeRow {
        id: row.get(0)?,
        hostname: row.get(1)?,
        address: row.get(2)?,
        cpu_cores: row.get(3)?,
        memory_bytes: row.get(4)?,
        status: row.get(5)?,
        last_heartbeat: row.get(6)?,
        gateway_interface: row.get(7)?,
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
    })
}

fn row_to_network(row: &rusqlite::Row) -> Result<NetworkRow, rusqlite::Error> {
    Ok(NetworkRow {
        name: row.get(0)?,
        external_ip: row.get(1)?,
        gateway_ip: row.get(2)?,
        internal_netmask: row.get(3)?,
        node_id: row.get(4)?,
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
}
