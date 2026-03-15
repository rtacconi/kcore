package node

import (
	"database/sql"
	"fmt"
	"time"

	_ "github.com/mattn/go-sqlite3"
)

// NodeDB stores local metadata that libvirt does not track (image URIs,
// cloud-init configs, operation history). Runtime state (running/stopped)
// is always queried from libvirt.
type NodeDB struct {
	conn *sql.DB
}

func NewNodeDB(path string) (*NodeDB, error) {
	conn, err := sql.Open("sqlite3", path+"?_journal_mode=WAL")
	if err != nil {
		return nil, fmt.Errorf("open node db: %w", err)
	}
	db := &NodeDB{conn: conn}
	if err := db.migrate(); err != nil {
		conn.Close()
		return nil, fmt.Errorf("migrate node db: %w", err)
	}
	return db, nil
}

func (db *NodeDB) Close() error { return db.conn.Close() }

// --- versioned migrations --------------------------------------------------

type nodeMigration struct {
	name string
	fn   func(*sql.Tx) error
}

var nodeMigrations = []nodeMigration{
	{"001_initial", nodeM001Initial},
}

func (db *NodeDB) migrate() error {
	if _, err := db.conn.Exec("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)"); err != nil {
		return err
	}
	current := db.currentVersion()
	for i := current; i < len(nodeMigrations); i++ {
		tx, err := db.conn.Begin()
		if err != nil {
			return fmt.Errorf("begin %s: %w", nodeMigrations[i].name, err)
		}
		if err := nodeMigrations[i].fn(tx); err != nil {
			tx.Rollback()
			return fmt.Errorf("%s: %w", nodeMigrations[i].name, err)
		}
		tx.Exec("DELETE FROM schema_version")
		tx.Exec("INSERT INTO schema_version (version) VALUES (?)", i+1)
		if err := tx.Commit(); err != nil {
			return fmt.Errorf("commit %s: %w", nodeMigrations[i].name, err)
		}
	}
	return nil
}

func (db *NodeDB) currentVersion() int {
	var v int
	if err := db.conn.QueryRow("SELECT version FROM schema_version LIMIT 1").Scan(&v); err != nil {
		return 0
	}
	return v
}

func (db *NodeDB) SchemaVersion() int { return db.currentVersion() }

func nodeM001Initial(tx *sql.Tx) error {
	_, err := tx.Exec(`
	CREATE TABLE IF NOT EXISTS vm_metadata (
		vm_id TEXT PRIMARY KEY,
		name TEXT NOT NULL,
		image_uri TEXT,
		cloud_init_config TEXT,
		created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
	);

	CREATE TABLE IF NOT EXISTS cached_images (
		uri TEXT PRIMARY KEY,
		local_path TEXT NOT NULL,
		size_bytes INTEGER NOT NULL DEFAULT 0,
		checksum TEXT,
		downloaded_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
	);

	CREATE TABLE IF NOT EXISTS operation_log (
		id INTEGER PRIMARY KEY AUTOINCREMENT,
		vm_id TEXT,
		operation TEXT NOT NULL,
		status TEXT NOT NULL,
		message TEXT,
		created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
	);

	CREATE INDEX IF NOT EXISTS idx_oplog_vm ON operation_log(vm_id);
	CREATE INDEX IF NOT EXISTS idx_oplog_time ON operation_log(created_at);
	`)
	return err
}

// --- VM metadata -----------------------------------------------------------

type VMMetadata struct {
	VMID            string
	Name            string
	ImageURI        string
	CloudInitConfig string
	CreatedAt       time.Time
}

func (db *NodeDB) SaveVMMetadata(m *VMMetadata) error {
	_, err := db.conn.Exec(`
		INSERT INTO vm_metadata (vm_id, name, image_uri, cloud_init_config)
		VALUES (?, ?, ?, ?)
		ON CONFLICT(vm_id) DO UPDATE SET
			name = excluded.name,
			image_uri = excluded.image_uri,
			cloud_init_config = excluded.cloud_init_config
	`, m.VMID, m.Name, m.ImageURI, m.CloudInitConfig)
	return err
}

func (db *NodeDB) GetVMMetadata(vmID string) (*VMMetadata, error) {
	row := db.conn.QueryRow(`SELECT vm_id, name, image_uri, cloud_init_config, created_at FROM vm_metadata WHERE vm_id = ?`, vmID)
	var m VMMetadata
	var imageURI, cloudInit sql.NullString
	if err := row.Scan(&m.VMID, &m.Name, &imageURI, &cloudInit, &m.CreatedAt); err != nil {
		return nil, err
	}
	if imageURI.Valid {
		m.ImageURI = imageURI.String
	}
	if cloudInit.Valid {
		m.CloudInitConfig = cloudInit.String
	}
	return &m, nil
}

func (db *NodeDB) DeleteVMMetadata(vmID string) error {
	_, err := db.conn.Exec(`DELETE FROM vm_metadata WHERE vm_id = ?`, vmID)
	return err
}

// --- cached images ---------------------------------------------------------

type CachedImage struct {
	URI          string
	LocalPath    string
	SizeBytes    int64
	Checksum     string
	DownloadedAt time.Time
}

func (db *NodeDB) SaveCachedImage(img *CachedImage) error {
	_, err := db.conn.Exec(`
		INSERT INTO cached_images (uri, local_path, size_bytes, checksum)
		VALUES (?, ?, ?, ?)
		ON CONFLICT(uri) DO UPDATE SET
			local_path = excluded.local_path,
			size_bytes = excluded.size_bytes,
			checksum = excluded.checksum,
			downloaded_at = CURRENT_TIMESTAMP
	`, img.URI, img.LocalPath, img.SizeBytes, img.Checksum)
	return err
}

func (db *NodeDB) GetCachedImage(uri string) (*CachedImage, error) {
	row := db.conn.QueryRow(`SELECT uri, local_path, size_bytes, checksum, downloaded_at FROM cached_images WHERE uri = ?`, uri)
	var img CachedImage
	var checksum sql.NullString
	if err := row.Scan(&img.URI, &img.LocalPath, &img.SizeBytes, &checksum, &img.DownloadedAt); err != nil {
		return nil, err
	}
	if checksum.Valid {
		img.Checksum = checksum.String
	}
	return &img, nil
}

func (db *NodeDB) ListCachedImages() ([]*CachedImage, error) {
	rows, err := db.conn.Query(`SELECT uri, local_path, size_bytes, checksum, downloaded_at FROM cached_images ORDER BY downloaded_at DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var images []*CachedImage
	for rows.Next() {
		var img CachedImage
		var checksum sql.NullString
		if err := rows.Scan(&img.URI, &img.LocalPath, &img.SizeBytes, &checksum, &img.DownloadedAt); err != nil {
			return nil, err
		}
		if checksum.Valid {
			img.Checksum = checksum.String
		}
		images = append(images, &img)
	}
	return images, rows.Err()
}

// --- operation log ---------------------------------------------------------

func (db *NodeDB) LogOperation(vmID, operation, opStatus, message string) error {
	_, err := db.conn.Exec(`INSERT INTO operation_log (vm_id, operation, status, message) VALUES (?, ?, ?, ?)`,
		vmID, operation, opStatus, message)
	return err
}
