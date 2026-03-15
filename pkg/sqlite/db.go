package sqlite

import (
	"database/sql"
	"fmt"
	"time"

	_ "github.com/mattn/go-sqlite3"
)

type DB struct {
	conn *sql.DB
}

func New(path string) (*DB, error) {
	conn, err := sql.Open("sqlite3", path+"?_foreign_keys=1&_journal_mode=WAL")
	if err != nil {
		return nil, fmt.Errorf("failed to open database: %w", err)
	}

	db := &DB{conn: conn}
	if err := db.migrate(); err != nil {
		conn.Close()
		return nil, fmt.Errorf("failed to migrate database: %w", err)
	}

	return db, nil
}

func (db *DB) Close() error {
	return db.conn.Close()
}

// --- Versioned migrations --------------------------------------------------

type migration struct {
	name string
	fn   func(*sql.Tx) error
}

var migrations = []migration{
	{"001_initial", migration001Initial},
	{"002_desired_state", migration002DesiredState},
}

func (db *DB) migrate() error {
	if _, err := db.conn.Exec("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)"); err != nil {
		return fmt.Errorf("failed to create schema_version table: %w", err)
	}

	current := db.currentVersion()

	for i := current; i < len(migrations); i++ {
		tx, err := db.conn.Begin()
		if err != nil {
			return fmt.Errorf("begin migration %s: %w", migrations[i].name, err)
		}
		if err := migrations[i].fn(tx); err != nil {
			tx.Rollback()
			return fmt.Errorf("migration %s failed: %w", migrations[i].name, err)
		}
		if _, err := tx.Exec("DELETE FROM schema_version"); err != nil {
			tx.Rollback()
			return err
		}
		if _, err := tx.Exec("INSERT INTO schema_version (version) VALUES (?)", i+1); err != nil {
			tx.Rollback()
			return err
		}
		if err := tx.Commit(); err != nil {
			return fmt.Errorf("commit migration %s: %w", migrations[i].name, err)
		}
	}
	return nil
}

func (db *DB) currentVersion() int {
	var v int
	if err := db.conn.QueryRow("SELECT version FROM schema_version LIMIT 1").Scan(&v); err != nil {
		return 0
	}
	return v
}

// SchemaVersion returns the current schema version (exported for tests).
func (db *DB) SchemaVersion() int {
	return db.currentVersion()
}

func migration001Initial(tx *sql.Tx) error {
	_, err := tx.Exec(`
	CREATE TABLE IF NOT EXISTS nodes (
		id TEXT PRIMARY KEY,
		hostname TEXT NOT NULL,
		address TEXT NOT NULL,
		cpu_cores INTEGER NOT NULL,
		memory_bytes INTEGER NOT NULL,
		labels TEXT,
		registered_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
		last_heartbeat TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
	);

	CREATE TABLE IF NOT EXISTS storage_classes (
		name TEXT PRIMARY KEY,
		driver TEXT NOT NULL,
		shared BOOLEAN NOT NULL DEFAULT 0,
		parameters TEXT
	);

	CREATE TABLE IF NOT EXISTS volumes (
		id TEXT PRIMARY KEY,
		name TEXT NOT NULL,
		namespace TEXT NOT NULL DEFAULT 'default',
		storage_class TEXT NOT NULL,
		size_bytes INTEGER NOT NULL,
		backend_handle TEXT,
		node_id TEXT,
		shared BOOLEAN NOT NULL,
		created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
		FOREIGN KEY (storage_class) REFERENCES storage_classes(name),
		FOREIGN KEY (node_id) REFERENCES nodes(id)
	);

	CREATE TABLE IF NOT EXISTS vms (
		id TEXT PRIMARY KEY,
		name TEXT NOT NULL,
		namespace TEXT NOT NULL DEFAULT 'default',
		cpu INTEGER NOT NULL,
		memory_bytes INTEGER NOT NULL,
		node_id TEXT,
		state TEXT NOT NULL DEFAULT 'pending',
		created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
		updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
		FOREIGN KEY (node_id) REFERENCES nodes(id)
	);

	CREATE TABLE IF NOT EXISTS vm_disks (
		vm_id TEXT NOT NULL,
		disk_name TEXT NOT NULL,
		volume_id TEXT NOT NULL,
		bus TEXT NOT NULL DEFAULT 'virtio',
		device TEXT NOT NULL,
		PRIMARY KEY (vm_id, disk_name),
		FOREIGN KEY (vm_id) REFERENCES vms(id) ON DELETE CASCADE,
		FOREIGN KEY (volume_id) REFERENCES volumes(id)
	);

	CREATE TABLE IF NOT EXISTS vm_nics (
		vm_id TEXT NOT NULL,
		network TEXT NOT NULL,
		model TEXT NOT NULL DEFAULT 'virtio',
		mac_address TEXT,
		PRIMARY KEY (vm_id, network),
		FOREIGN KEY (vm_id) REFERENCES vms(id) ON DELETE CASCADE
	);

	CREATE TABLE IF NOT EXISTS vm_placement (
		vm_id TEXT PRIMARY KEY,
		desired_node_id TEXT,
		actual_node_id TEXT,
		desired_state TEXT NOT NULL DEFAULT 'stopped',
		actual_state TEXT NOT NULL DEFAULT 'unknown',
		FOREIGN KEY (vm_id) REFERENCES vms(id) ON DELETE CASCADE,
		FOREIGN KEY (desired_node_id) REFERENCES nodes(id),
		FOREIGN KEY (actual_node_id) REFERENCES nodes(id)
	);

	CREATE TABLE IF NOT EXISTS networks (
		name TEXT PRIMARY KEY,
		bridge_name TEXT NOT NULL,
		description TEXT
	);

	CREATE INDEX IF NOT EXISTS idx_nodes_labels ON nodes(labels);
	CREATE INDEX IF NOT EXISTS idx_volumes_storage_class ON volumes(storage_class);
	CREATE INDEX IF NOT EXISTS idx_volumes_node_id ON volumes(node_id);
	CREATE INDEX IF NOT EXISTS idx_vms_node_id ON vms(node_id);
	CREATE INDEX IF NOT EXISTS idx_vms_state ON vms(state);
	CREATE INDEX IF NOT EXISTS idx_vm_placement_desired_node ON vm_placement(desired_node_id);
	CREATE INDEX IF NOT EXISTS idx_vm_placement_actual_node ON vm_placement(actual_node_id);
	`)
	return err
}

func migration002DesiredState(tx *sql.Tx) error {
	stmts := []string{
		`ALTER TABLE vms ADD COLUMN desired_spec TEXT`,
		`ALTER TABLE vms ADD COLUMN desired_state TEXT NOT NULL DEFAULT 'running'`,
		`ALTER TABLE vms ADD COLUMN image_uri TEXT`,
	}
	for _, s := range stmts {
		if _, err := tx.Exec(s); err != nil {
			return err
		}
	}
	return nil
}

// --- Node operations -------------------------------------------------------

type Node struct {
	ID            string
	Hostname      string
	Address       string
	CPUCores      int
	MemoryBytes   int64
	Labels        []string
	RegisteredAt  time.Time
	LastHeartbeat time.Time
}

func (db *DB) UpsertNode(node *Node) error {
	labelsJSON := `[]`
	if len(node.Labels) > 0 {
		labelsJSON = fmt.Sprintf(`["%s"]`, node.Labels[0])
		for i := 1; i < len(node.Labels); i++ {
			labelsJSON = fmt.Sprintf(`%s,"%s"`, labelsJSON, node.Labels[i])
		}
		labelsJSON = "[" + labelsJSON + "]"
	}

	query := `
		INSERT INTO nodes (id, hostname, address, cpu_cores, memory_bytes, labels, registered_at, last_heartbeat)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT(id) DO UPDATE SET
			hostname = excluded.hostname,
			address = excluded.address,
			cpu_cores = excluded.cpu_cores,
			memory_bytes = excluded.memory_bytes,
			labels = excluded.labels,
			last_heartbeat = excluded.last_heartbeat
	`

	now := time.Now()
	_, err := db.conn.Exec(query, node.ID, node.Hostname, node.Address, node.CPUCores, node.MemoryBytes, labelsJSON, now, now)
	return err
}

func (db *DB) GetNode(id string) (*Node, error) {
	query := `SELECT id, hostname, address, cpu_cores, memory_bytes, labels, registered_at, last_heartbeat FROM nodes WHERE id = ?`
	row := db.conn.QueryRow(query, id)

	var node Node
	var labelsJSON string
	err := row.Scan(&node.ID, &node.Hostname, &node.Address, &node.CPUCores, &node.MemoryBytes, &labelsJSON, &node.RegisteredAt, &node.LastHeartbeat)
	if err != nil {
		return nil, err
	}

	node.Labels = []string{}
	return &node, nil
}

func (db *DB) ListNodes() ([]*Node, error) {
	query := `SELECT id, hostname, address, cpu_cores, memory_bytes, labels, registered_at, last_heartbeat FROM nodes`
	rows, err := db.conn.Query(query)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var nodes []*Node
	for rows.Next() {
		var node Node
		var labelsJSON string
		if err := rows.Scan(&node.ID, &node.Hostname, &node.Address, &node.CPUCores, &node.MemoryBytes, &labelsJSON, &node.RegisteredAt, &node.LastHeartbeat); err != nil {
			return nil, err
		}
		node.Labels = []string{}
		nodes = append(nodes, &node)
	}
	return nodes, rows.Err()
}

func (db *DB) GetNodeByAddress(address string) (*Node, error) {
	query := `SELECT id, hostname, address, cpu_cores, memory_bytes, labels, registered_at, last_heartbeat FROM nodes WHERE address = ?`
	row := db.conn.QueryRow(query, address)

	var node Node
	var labelsJSON string
	err := row.Scan(&node.ID, &node.Hostname, &node.Address, &node.CPUCores, &node.MemoryBytes, &labelsJSON, &node.RegisteredAt, &node.LastHeartbeat)
	if err != nil {
		return nil, err
	}
	node.Labels = []string{}
	return &node, nil
}

func (db *DB) UpdateNodeHeartbeat(nodeID string) error {
	_, err := db.conn.Exec(`UPDATE nodes SET last_heartbeat = ? WHERE id = ?`, time.Now(), nodeID)
	return err
}

// --- StorageClass operations -----------------------------------------------

type StorageClass struct {
	Name       string
	Driver     string
	Shared     bool
	Parameters map[string]string
}

func (db *DB) CreateStorageClass(sc *StorageClass) error {
	paramsJSON := `{}`
	query := `INSERT INTO storage_classes (name, driver, shared, parameters) VALUES (?, ?, ?, ?)`
	_, err := db.conn.Exec(query, sc.Name, sc.Driver, sc.Shared, paramsJSON)
	return err
}

func (db *DB) GetStorageClass(name string) (*StorageClass, error) {
	query := `SELECT name, driver, shared, parameters FROM storage_classes WHERE name = ?`
	row := db.conn.QueryRow(query, name)

	var sc StorageClass
	var paramsJSON string
	err := row.Scan(&sc.Name, &sc.Driver, &sc.Shared, &paramsJSON)
	if err != nil {
		return nil, err
	}
	sc.Parameters = make(map[string]string)
	return &sc, nil
}

// --- Volume operations -----------------------------------------------------

type Volume struct {
	ID            string
	Name          string
	Namespace     string
	StorageClass  string
	SizeBytes     int64
	BackendHandle string
	NodeID        *string
	Shared        bool
	CreatedAt     time.Time
}

func (db *DB) CreateVolume(vol *Volume) error {
	query := `
		INSERT INTO volumes (id, name, namespace, storage_class, size_bytes, backend_handle, node_id, shared)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?)
	`
	_, err := db.conn.Exec(query, vol.ID, vol.Name, vol.Namespace, vol.StorageClass, vol.SizeBytes, vol.BackendHandle, vol.NodeID, vol.Shared)
	return err
}

func (db *DB) UpdateVolumeBackendHandle(volumeID, backendHandle string) error {
	query := `UPDATE volumes SET backend_handle = ? WHERE id = ?`
	_, err := db.conn.Exec(query, backendHandle, volumeID)
	return err
}

func (db *DB) GetVolume(id string) (*Volume, error) {
	query := `SELECT id, name, namespace, storage_class, size_bytes, backend_handle, node_id, shared, created_at FROM volumes WHERE id = ?`
	row := db.conn.QueryRow(query, id)

	var vol Volume
	var nodeID sql.NullString
	err := row.Scan(&vol.ID, &vol.Name, &vol.Namespace, &vol.StorageClass, &vol.SizeBytes, &vol.BackendHandle, &nodeID, &vol.Shared, &vol.CreatedAt)
	if err != nil {
		return nil, err
	}
	if nodeID.Valid {
		vol.NodeID = &nodeID.String
	}
	return &vol, nil
}

// --- VM operations ---------------------------------------------------------

type VM struct {
	ID           string
	Name         string
	Namespace    string
	CPU          int
	MemoryBytes  int64
	NodeID       *string
	State        string // actual state reported by node-agent
	DesiredSpec  string // JSON-encoded user-submitted spec
	DesiredState string // what the user wants: running, stopped, deleted
	ImageURI     string
	CreatedAt    time.Time
	UpdatedAt    time.Time
}

type VMDisk struct {
	VMID     string
	DiskName string
	VolumeID string
	Bus      string
	Device   string
}

type VMNIC struct {
	VMID       string
	Network    string
	Model      string
	MACAddress *string
}

func (db *DB) CreateVM(vm *VM) error {
	tx, err := db.conn.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()

	query := `
		INSERT INTO vms (id, name, namespace, cpu, memory_bytes, node_id, state, desired_spec, desired_state, image_uri)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`
	desiredState := vm.DesiredState
	if desiredState == "" {
		desiredState = "running"
	}
	_, err = tx.Exec(query, vm.ID, vm.Name, vm.Namespace, vm.CPU, vm.MemoryBytes, vm.NodeID, vm.State, vm.DesiredSpec, desiredState, vm.ImageURI)
	if err != nil {
		return err
	}

	placementQuery := `
		INSERT INTO vm_placement (vm_id, desired_node_id, actual_node_id, desired_state, actual_state)
		VALUES (?, ?, ?, ?, ?)
	`
	_, err = tx.Exec(placementQuery, vm.ID, vm.NodeID, nil, "stopped", "unknown")
	if err != nil {
		return err
	}

	return tx.Commit()
}

func (db *DB) UpdateVMState(vmID, state string) error {
	query := `UPDATE vms SET state = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?`
	_, err := db.conn.Exec(query, state, vmID)
	return err
}

func (db *DB) UpdateVMDesiredState(vmID, desiredState string) error {
	query := `UPDATE vms SET desired_state = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?`
	_, err := db.conn.Exec(query, desiredState, vmID)
	return err
}

func (db *DB) UpdateVMNodeID(vmID, nodeID string) error {
	query := `UPDATE vms SET node_id = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?`
	_, err := db.conn.Exec(query, nodeID, vmID)
	return err
}

func (db *DB) GetVM(id string) (*VM, error) {
	query := `SELECT id, name, namespace, cpu, memory_bytes, node_id, state, desired_spec, desired_state, image_uri, created_at, updated_at FROM vms WHERE id = ?`
	row := db.conn.QueryRow(query, id)
	return scanVM(row)
}

func (db *DB) GetVMByName(name string) (*VM, error) {
	query := `SELECT id, name, namespace, cpu, memory_bytes, node_id, state, desired_spec, desired_state, image_uri, created_at, updated_at FROM vms WHERE name = ?`
	row := db.conn.QueryRow(query, name)
	return scanVM(row)
}

func (db *DB) ListVMs() ([]*VM, error) {
	query := `SELECT id, name, namespace, cpu, memory_bytes, node_id, state, desired_spec, desired_state, image_uri, created_at, updated_at FROM vms WHERE desired_state != 'deleted' ORDER BY created_at DESC`
	rows, err := db.conn.Query(query)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var vms []*VM
	for rows.Next() {
		vm, err := scanVMRow(rows)
		if err != nil {
			return nil, err
		}
		vms = append(vms, vm)
	}
	return vms, rows.Err()
}

func (db *DB) DeleteVM(id string) error {
	_, err := db.conn.Exec(`DELETE FROM vms WHERE id = ?`, id)
	return err
}

type scanner interface {
	Scan(dest ...interface{}) error
}

func scanVM(row scanner) (*VM, error) {
	var vm VM
	var nodeID sql.NullString
	var desiredSpec, desiredState, imageURI sql.NullString
	err := row.Scan(&vm.ID, &vm.Name, &vm.Namespace, &vm.CPU, &vm.MemoryBytes, &nodeID,
		&vm.State, &desiredSpec, &desiredState, &imageURI, &vm.CreatedAt, &vm.UpdatedAt)
	if err != nil {
		return nil, err
	}
	if nodeID.Valid {
		vm.NodeID = &nodeID.String
	}
	if desiredSpec.Valid {
		vm.DesiredSpec = desiredSpec.String
	}
	if desiredState.Valid {
		vm.DesiredState = desiredState.String
	}
	if imageURI.Valid {
		vm.ImageURI = imageURI.String
	}
	return &vm, nil
}

func scanVMRow(rows *sql.Rows) (*VM, error) {
	var vm VM
	var nodeID sql.NullString
	var desiredSpec, desiredState, imageURI sql.NullString
	err := rows.Scan(&vm.ID, &vm.Name, &vm.Namespace, &vm.CPU, &vm.MemoryBytes, &nodeID,
		&vm.State, &desiredSpec, &desiredState, &imageURI, &vm.CreatedAt, &vm.UpdatedAt)
	if err != nil {
		return nil, err
	}
	if nodeID.Valid {
		vm.NodeID = &nodeID.String
	}
	if desiredSpec.Valid {
		vm.DesiredSpec = desiredSpec.String
	}
	if desiredState.Valid {
		vm.DesiredState = desiredState.String
	}
	if imageURI.Valid {
		vm.ImageURI = imageURI.String
	}
	return &vm, nil
}

func (db *DB) AddVMDisk(disk *VMDisk) error {
	query := `INSERT INTO vm_disks (vm_id, disk_name, volume_id, bus, device) VALUES (?, ?, ?, ?, ?)`
	_, err := db.conn.Exec(query, disk.VMID, disk.DiskName, disk.VolumeID, disk.Bus, disk.Device)
	return err
}

func (db *DB) AddVMNIC(nic *VMNIC) error {
	query := `INSERT INTO vm_nics (vm_id, network, model, mac_address) VALUES (?, ?, ?, ?)`
	_, err := db.conn.Exec(query, nic.VMID, nic.Network, nic.Model, nic.MACAddress)
	return err
}

func (db *DB) GetVMDisks(vmID string) ([]*VMDisk, error) {
	query := `SELECT vm_id, disk_name, volume_id, bus, device FROM vm_disks WHERE vm_id = ? ORDER BY device`
	rows, err := db.conn.Query(query, vmID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var disks []*VMDisk
	for rows.Next() {
		var disk VMDisk
		if err := rows.Scan(&disk.VMID, &disk.DiskName, &disk.VolumeID, &disk.Bus, &disk.Device); err != nil {
			return nil, err
		}
		disks = append(disks, &disk)
	}
	return disks, rows.Err()
}

func (db *DB) GetVMNICs(vmID string) ([]*VMNIC, error) {
	query := `SELECT vm_id, network, model, mac_address FROM vm_nics WHERE vm_id = ?`
	rows, err := db.conn.Query(query, vmID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var nics []*VMNIC
	for rows.Next() {
		var nic VMNIC
		var macAddress sql.NullString
		if err := rows.Scan(&nic.VMID, &nic.Network, &nic.Model, &macAddress); err != nil {
			return nil, err
		}
		if macAddress.Valid {
			nic.MACAddress = &macAddress.String
		}
		nics = append(nics, &nic)
	}
	return nics, rows.Err()
}

// --- VM Placement operations -----------------------------------------------

type VMPlacement struct {
	VMID          string
	DesiredNodeID *string
	ActualNodeID  *string
	DesiredState  string
	ActualState   string
}

func (db *DB) UpdateVMPlacement(placement *VMPlacement) error {
	query := `
		UPDATE vm_placement
		SET desired_node_id = ?, actual_node_id = ?, desired_state = ?, actual_state = ?
		WHERE vm_id = ?
	`
	_, err := db.conn.Exec(query, placement.DesiredNodeID, placement.ActualNodeID, placement.DesiredState, placement.ActualState, placement.VMID)
	return err
}

func (db *DB) GetVMPlacement(vmID string) (*VMPlacement, error) {
	query := `SELECT vm_id, desired_node_id, actual_node_id, desired_state, actual_state FROM vm_placement WHERE vm_id = ?`
	row := db.conn.QueryRow(query, vmID)

	var placement VMPlacement
	var desiredNodeID, actualNodeID sql.NullString
	err := row.Scan(&placement.VMID, &desiredNodeID, &actualNodeID, &placement.DesiredState, &placement.ActualState)
	if err != nil {
		return nil, err
	}
	if desiredNodeID.Valid {
		placement.DesiredNodeID = &desiredNodeID.String
	}
	if actualNodeID.Valid {
		placement.ActualNodeID = &actualNodeID.String
	}
	return &placement, nil
}

func (db *DB) ListVMsForReconciliation() ([]*VMPlacement, error) {
	query := `
		SELECT vm_id, desired_node_id, actual_node_id, desired_state, actual_state
		FROM vm_placement
		WHERE desired_state != actual_state OR desired_node_id != actual_node_id
	`
	rows, err := db.conn.Query(query)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var placements []*VMPlacement
	for rows.Next() {
		var placement VMPlacement
		var desiredNodeID, actualNodeID sql.NullString
		if err := rows.Scan(&placement.VMID, &desiredNodeID, &actualNodeID, &placement.DesiredState, &placement.ActualState); err != nil {
			return nil, err
		}
		if desiredNodeID.Valid {
			placement.DesiredNodeID = &desiredNodeID.String
		}
		if actualNodeID.Valid {
			placement.ActualNodeID = &actualNodeID.String
		}
		placements = append(placements, &placement)
	}
	return placements, rows.Err()
}
