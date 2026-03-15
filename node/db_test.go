package node

import (
	"testing"
)

func newTestNodeDB(t *testing.T) *NodeDB {
	t.Helper()
	db, err := NewNodeDB(":memory:")
	if err != nil {
		t.Fatalf("NewNodeDB: %v", err)
	}
	t.Cleanup(func() { db.Close() })
	return db
}

func TestNewNodeDB(t *testing.T) {
	db := newTestNodeDB(t)
	if db == nil {
		t.Fatal("db is nil")
	}
	if v := db.SchemaVersion(); v != 1 {
		t.Errorf("SchemaVersion()=%d, want 1", v)
	}
}

func TestNodeDB_MigrationsIdempotent(t *testing.T) {
	db := newTestNodeDB(t)
	v1 := db.SchemaVersion()
	if err := db.migrate(); err != nil {
		t.Fatalf("re-migrate: %v", err)
	}
	if v2 := db.SchemaVersion(); v1 != v2 {
		t.Errorf("version changed: %d -> %d", v1, v2)
	}
}

func TestNodeDB_VMMetadata(t *testing.T) {
	db := newTestNodeDB(t)

	m := &VMMetadata{
		VMID:            "vm-123",
		Name:            "test-vm",
		ImageURI:        "https://example.com/image.qcow2",
		CloudInitConfig: "#cloud-config\nusers: []",
	}
	if err := db.SaveVMMetadata(m); err != nil {
		t.Fatalf("SaveVMMetadata: %v", err)
	}

	got, err := db.GetVMMetadata("vm-123")
	if err != nil {
		t.Fatalf("GetVMMetadata: %v", err)
	}
	if got.Name != "test-vm" {
		t.Errorf("Name=%q", got.Name)
	}
	if got.ImageURI != "https://example.com/image.qcow2" {
		t.Errorf("ImageURI=%q", got.ImageURI)
	}
	if got.CloudInitConfig != "#cloud-config\nusers: []" {
		t.Errorf("CloudInitConfig=%q", got.CloudInitConfig)
	}
}

func TestNodeDB_VMMetadata_Upsert(t *testing.T) {
	db := newTestNodeDB(t)

	db.SaveVMMetadata(&VMMetadata{VMID: "vm-1", Name: "old-name"})
	db.SaveVMMetadata(&VMMetadata{VMID: "vm-1", Name: "new-name"})

	got, err := db.GetVMMetadata("vm-1")
	if err != nil {
		t.Fatalf("GetVMMetadata: %v", err)
	}
	if got.Name != "new-name" {
		t.Errorf("Name=%q, want new-name", got.Name)
	}
}

func TestNodeDB_DeleteVMMetadata(t *testing.T) {
	db := newTestNodeDB(t)

	db.SaveVMMetadata(&VMMetadata{VMID: "vm-1", Name: "to-delete"})
	if err := db.DeleteVMMetadata("vm-1"); err != nil {
		t.Fatalf("DeleteVMMetadata: %v", err)
	}

	_, err := db.GetVMMetadata("vm-1")
	if err == nil {
		t.Error("expected error after deletion")
	}
}

func TestNodeDB_CachedImages(t *testing.T) {
	db := newTestNodeDB(t)

	img := &CachedImage{
		URI:       "https://example.com/debian.qcow2",
		LocalPath: "/var/lib/kcore/images/debian.qcow2",
		SizeBytes: 500 * 1024 * 1024,
		Checksum:  "sha256:abc123",
	}
	if err := db.SaveCachedImage(img); err != nil {
		t.Fatalf("SaveCachedImage: %v", err)
	}

	got, err := db.GetCachedImage("https://example.com/debian.qcow2")
	if err != nil {
		t.Fatalf("GetCachedImage: %v", err)
	}
	if got.LocalPath != "/var/lib/kcore/images/debian.qcow2" {
		t.Errorf("LocalPath=%q", got.LocalPath)
	}
	if got.SizeBytes != 500*1024*1024 {
		t.Errorf("SizeBytes=%d", got.SizeBytes)
	}
	if got.Checksum != "sha256:abc123" {
		t.Errorf("Checksum=%q", got.Checksum)
	}
}

func TestNodeDB_ListCachedImages(t *testing.T) {
	db := newTestNodeDB(t)

	db.SaveCachedImage(&CachedImage{URI: "https://a.com/1.qcow2", LocalPath: "/a", SizeBytes: 100})
	db.SaveCachedImage(&CachedImage{URI: "https://b.com/2.qcow2", LocalPath: "/b", SizeBytes: 200})

	images, err := db.ListCachedImages()
	if err != nil {
		t.Fatalf("ListCachedImages: %v", err)
	}
	if len(images) != 2 {
		t.Errorf("count=%d, want 2", len(images))
	}
}

func TestNodeDB_OperationLog(t *testing.T) {
	db := newTestNodeDB(t)

	if err := db.LogOperation("vm-1", "create", "success", "VM created"); err != nil {
		t.Fatalf("LogOperation: %v", err)
	}
	if err := db.LogOperation("vm-1", "start", "success", "VM started"); err != nil {
		t.Fatalf("LogOperation: %v", err)
	}
}

func TestNodeDB_GetVMMetadata_NotFound(t *testing.T) {
	db := newTestNodeDB(t)

	_, err := db.GetVMMetadata("nonexistent")
	if err == nil {
		t.Fatal("expected error for missing VM metadata")
	}
}

func TestNodeDB_GetCachedImage_NotFound(t *testing.T) {
	db := newTestNodeDB(t)

	_, err := db.GetCachedImage("https://nonexistent.com/image.qcow2")
	if err == nil {
		t.Fatal("expected error for missing cached image")
	}
}

func TestNodeDB_CachedImage_Upsert(t *testing.T) {
	db := newTestNodeDB(t)

	db.SaveCachedImage(&CachedImage{URI: "https://a.com/1.qcow2", LocalPath: "/old", SizeBytes: 100})
	db.SaveCachedImage(&CachedImage{URI: "https://a.com/1.qcow2", LocalPath: "/new", SizeBytes: 200, Checksum: "sha256:updated"})

	got, err := db.GetCachedImage("https://a.com/1.qcow2")
	if err != nil {
		t.Fatalf("GetCachedImage: %v", err)
	}
	if got.LocalPath != "/new" {
		t.Errorf("LocalPath=%q, want /new", got.LocalPath)
	}
	if got.SizeBytes != 200 {
		t.Errorf("SizeBytes=%d, want 200", got.SizeBytes)
	}
	if got.Checksum != "sha256:updated" {
		t.Errorf("Checksum=%q", got.Checksum)
	}
}

func TestNodeDB_ListCachedImages_Empty(t *testing.T) {
	db := newTestNodeDB(t)

	images, err := db.ListCachedImages()
	if err != nil {
		t.Fatalf("ListCachedImages: %v", err)
	}
	if len(images) != 0 {
		t.Errorf("expected 0 images, got %d", len(images))
	}
}

func TestNodeDB_VMMetadata_EmptyOptionals(t *testing.T) {
	db := newTestNodeDB(t)

	m := &VMMetadata{VMID: "vm-empty", Name: "empty-vm"}
	if err := db.SaveVMMetadata(m); err != nil {
		t.Fatalf("SaveVMMetadata: %v", err)
	}

	got, err := db.GetVMMetadata("vm-empty")
	if err != nil {
		t.Fatalf("GetVMMetadata: %v", err)
	}
	if got.ImageURI != "" {
		t.Errorf("ImageURI=%q, want empty", got.ImageURI)
	}
	if got.CloudInitConfig != "" {
		t.Errorf("CloudInitConfig=%q, want empty", got.CloudInitConfig)
	}
}

func TestNodeDB_CachedImage_NullChecksum(t *testing.T) {
	db := newTestNodeDB(t)

	img := &CachedImage{URI: "https://a.com/img.qcow2", LocalPath: "/path", SizeBytes: 100}
	if err := db.SaveCachedImage(img); err != nil {
		t.Fatalf("SaveCachedImage: %v", err)
	}

	got, err := db.GetCachedImage("https://a.com/img.qcow2")
	if err != nil {
		t.Fatalf("GetCachedImage: %v", err)
	}
	if got.Checksum != "" {
		t.Errorf("Checksum=%q, want empty", got.Checksum)
	}
}

func TestNodeDB_OperationLog_MultipleOps(t *testing.T) {
	db := newTestNodeDB(t)

	ops := []struct {
		vmID, op, status, msg string
	}{
		{"vm-1", "create", "success", "created"},
		{"vm-1", "start", "success", "started"},
		{"vm-2", "create", "error", "disk full"},
		{"vm-1", "stop", "success", "stopped"},
	}
	for _, o := range ops {
		if err := db.LogOperation(o.vmID, o.op, o.status, o.msg); err != nil {
			t.Fatalf("LogOperation(%s, %s): %v", o.vmID, o.op, err)
		}
	}
}

func TestNodeDB_DeleteVMMetadata_Nonexistent(t *testing.T) {
	db := newTestNodeDB(t)

	err := db.DeleteVMMetadata("nonexistent")
	if err != nil {
		t.Fatalf("DeleteVMMetadata should not error for nonexistent: %v", err)
	}
}
