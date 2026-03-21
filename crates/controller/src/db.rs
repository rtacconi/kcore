use std::sync::{Arc, Mutex};

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
    pub image_size: i64,
    pub network: String,
    pub auto_start: bool,
    pub node_id: String,
    #[allow(dead_code)]
    pub created_at: String,
}

impl Database {
    pub fn open(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
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

    fn migrate(&self) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();
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
                image_size INTEGER NOT NULL DEFAULT 8192,
                network TEXT NOT NULL DEFAULT 'default',
                auto_start INTEGER NOT NULL DEFAULT 1,
                node_id TEXT NOT NULL REFERENCES nodes(id),
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )?;
        Ok(())
    }

    pub fn upsert_node(&self, node: &NodeRow) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface FROM nodes WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![node_id], row_to_node)?;
        rows.next().transpose()
    }

    pub fn list_nodes(&self) -> Result<Vec<NodeRow>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface FROM nodes",
        )?;
        let rows = stmt.query_map([], row_to_node)?;
        rows.collect()
    }

    pub fn get_node_by_address(&self, address: &str) -> Result<Option<NodeRow>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, hostname, address, cpu_cores, memory_bytes, status, last_heartbeat, gateway_interface FROM nodes WHERE address = ?1",
        )?;
        let mut rows = stmt.query_map(params![address], row_to_node)?;
        rows.next().transpose()
    }

    pub fn insert_vm(&self, vm: &VmRow) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO vms (id, name, cpu, memory_bytes, image_path, image_size, network, auto_start, node_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))",
            params![
                vm.id,
                vm.name,
                vm.cpu,
                vm.memory_bytes,
                vm.image_path,
                vm.image_size,
                vm.network,
                vm.auto_start as i32,
                vm.node_id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_vm(&self, vm_id: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute("DELETE FROM vms WHERE id = ?1", params![vm_id])?;
        Ok(rows > 0)
    }

    #[allow(dead_code)]
    pub fn get_vm(&self, vm_id: &str) -> Result<Option<VmRow>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, cpu, memory_bytes, image_path, image_size, network, auto_start, node_id, created_at FROM vms WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![vm_id], row_to_vm)?;
        rows.next().transpose()
    }

    #[allow(dead_code)]
    pub fn list_vms(&self) -> Result<Vec<VmRow>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, cpu, memory_bytes, image_path, image_size, network, auto_start, node_id, created_at FROM vms",
        )?;
        let rows = stmt.query_map([], row_to_vm)?;
        rows.collect()
    }

    pub fn list_vms_for_node(&self, node_id: &str) -> Result<Vec<VmRow>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, cpu, memory_bytes, image_path, image_size, network, auto_start, node_id, created_at FROM vms WHERE node_id = ?1",
        )?;
        let rows = stmt.query_map(params![node_id], row_to_vm)?;
        rows.collect()
    }

    pub fn find_node_for_vm(&self, vm_id: &str) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT node_id FROM vms WHERE id = ?1 OR name = ?1 LIMIT 1")?;
        let mut rows = stmt.query_map(params![vm_id], |row| row.get::<_, String>(0))?;
        rows.next().transpose()
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
    Ok(VmRow {
        id: row.get(0)?,
        name: row.get(1)?,
        cpu: row.get(2)?,
        memory_bytes: row.get(3)?,
        image_path: row.get(4)?,
        image_size: row.get(5)?,
        network: row.get(6)?,
        auto_start: row.get::<_, i32>(7)? != 0,
        node_id: row.get(8)?,
        created_at: row.get(9)?,
    })
}
