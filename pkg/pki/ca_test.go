package pki

import (
	"crypto/ecdsa"
	"crypto/x509"
	"encoding/pem"
	"os"
	"path/filepath"
	"testing"
)

func TestGenerateCA(t *testing.T) {
	mgr := &CAManager{BasePath: t.TempDir()}

	if err := mgr.GenerateCA("test-cluster"); err != nil {
		t.Fatalf("GenerateCA: %v", err)
	}

	dir := filepath.Join(mgr.BasePath, "test-cluster")
	assertFileExists(t, filepath.Join(dir, "ca.crt"))
	assertFileExists(t, filepath.Join(dir, "ca.key"))

	cert := loadTestCert(t, filepath.Join(dir, "ca.crt"))
	if cert.Subject.CommonName != "kcore-ca" {
		t.Errorf("CA CN = %q, want kcore-ca", cert.Subject.CommonName)
	}
	if !cert.IsCA {
		t.Error("CA cert is not marked as CA")
	}
}

func TestSignNodeCert(t *testing.T) {
	mgr := &CAManager{BasePath: t.TempDir()}
	if err := mgr.GenerateCA("test-cluster"); err != nil {
		t.Fatalf("GenerateCA: %v", err)
	}

	certPEM, keyPEM, err := mgr.SignNodeCert("test-cluster", "node-01", "192.168.1.100")
	if err != nil {
		t.Fatalf("SignNodeCert: %v", err)
	}

	cert := parsePEMCert(t, certPEM)
	if cert.Subject.CommonName != "kcore-node-node-01" {
		t.Errorf("cert CN = %q, want kcore-node-node-01", cert.Subject.CommonName)
	}

	hasServerAuth := false
	hasClientAuth := false
	for _, u := range cert.ExtKeyUsage {
		if u == x509.ExtKeyUsageServerAuth {
			hasServerAuth = true
		}
		if u == x509.ExtKeyUsageClientAuth {
			hasClientAuth = true
		}
	}
	if !hasServerAuth {
		t.Error("node cert missing ServerAuth")
	}
	if !hasClientAuth {
		t.Error("node cert missing ClientAuth")
	}

	foundIP := false
	for _, ip := range cert.IPAddresses {
		if ip.String() == "192.168.1.100" {
			foundIP = true
		}
	}
	if !foundIP {
		t.Error("node cert missing IP SAN 192.168.1.100")
	}

	// Verify the key is valid
	block, _ := pem.Decode(keyPEM)
	if block == nil {
		t.Fatal("no PEM block in key")
	}
	if _, err := x509.ParseECPrivateKey(block.Bytes); err != nil {
		t.Fatalf("parse key: %v", err)
	}

	// Verify cert is signed by CA
	caCert := loadTestCert(t, filepath.Join(mgr.BasePath, "test-cluster", "ca.crt"))
	pool := x509.NewCertPool()
	pool.AddCert(caCert)
	if _, err := cert.Verify(x509.VerifyOptions{Roots: pool, KeyUsages: []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth}}); err != nil {
		t.Fatalf("cert verification failed: %v", err)
	}
}

func TestSignControllerCert(t *testing.T) {
	mgr := &CAManager{BasePath: t.TempDir()}
	if err := mgr.GenerateCA("test-cluster"); err != nil {
		t.Fatalf("GenerateCA: %v", err)
	}

	certPEM, _, err := mgr.SignControllerCert("test-cluster", "10.0.0.1")
	if err != nil {
		t.Fatalf("SignControllerCert: %v", err)
	}

	cert := parsePEMCert(t, certPEM)
	if cert.Subject.CommonName != "kcore-controller" {
		t.Errorf("cert CN = %q, want kcore-controller", cert.Subject.CommonName)
	}
}

func TestWriteNodeCerts(t *testing.T) {
	mgr := &CAManager{BasePath: t.TempDir()}
	if err := mgr.GenerateCA("test-cluster"); err != nil {
		t.Fatalf("GenerateCA: %v", err)
	}

	outDir := filepath.Join(t.TempDir(), "node-certs")
	if err := mgr.WriteNodeCerts("test-cluster", "node-01", "192.168.1.100", outDir); err != nil {
		t.Fatalf("WriteNodeCerts: %v", err)
	}

	assertFileExists(t, filepath.Join(outDir, "node.crt"))
	assertFileExists(t, filepath.Join(outDir, "node.key"))
	assertFileExists(t, filepath.Join(outDir, "ca.crt"))
}

func TestWriteControllerCerts(t *testing.T) {
	mgr := &CAManager{BasePath: t.TempDir()}
	if err := mgr.GenerateCA("test-cluster"); err != nil {
		t.Fatalf("GenerateCA: %v", err)
	}

	outDir := filepath.Join(t.TempDir(), "ctrl-certs")
	if err := mgr.WriteControllerCerts("test-cluster", "10.0.0.1", outDir); err != nil {
		t.Fatalf("WriteControllerCerts: %v", err)
	}

	assertFileExists(t, filepath.Join(outDir, "controller.crt"))
	assertFileExists(t, filepath.Join(outDir, "controller.key"))
	assertFileExists(t, filepath.Join(outDir, "ca.crt"))
}

func TestClusterExists(t *testing.T) {
	mgr := &CAManager{BasePath: t.TempDir()}

	if mgr.ClusterExists("nonexistent") {
		t.Error("ClusterExists returned true for nonexistent cluster")
	}

	if err := mgr.GenerateCA("existing"); err != nil {
		t.Fatalf("GenerateCA: %v", err)
	}
	if !mgr.ClusterExists("existing") {
		t.Error("ClusterExists returned false for existing cluster")
	}
}

func TestSignCert_NoCA(t *testing.T) {
	mgr := &CAManager{BasePath: t.TempDir()}
	_, _, err := mgr.SignNodeCert("no-cluster", "node-01", "1.2.3.4")
	if err == nil {
		t.Error("expected error when signing without CA")
	}
}

func TestNewCAManager_DefaultPath(t *testing.T) {
	mgr, err := NewCAManager("")
	if err != nil {
		t.Fatalf("NewCAManager: %v", err)
	}
	home, _ := os.UserHomeDir()
	expected := filepath.Join(home, ".kcore", "clusters")
	if mgr.BasePath != expected {
		t.Errorf("BasePath = %q, want %q", mgr.BasePath, expected)
	}
}

func TestNewCAManager_CustomPath(t *testing.T) {
	mgr, err := NewCAManager("/tmp/test-pki")
	if err != nil {
		t.Fatalf("NewCAManager: %v", err)
	}
	if mgr.BasePath != "/tmp/test-pki" {
		t.Errorf("BasePath = %q, want /tmp/test-pki", mgr.BasePath)
	}
}

func TestLoadCACert(t *testing.T) {
	mgr := &CAManager{BasePath: t.TempDir()}
	if err := mgr.GenerateCA("test-cluster"); err != nil {
		t.Fatalf("GenerateCA: %v", err)
	}

	data, err := mgr.LoadCACert("test-cluster")
	if err != nil {
		t.Fatalf("LoadCACert: %v", err)
	}

	block, _ := pem.Decode(data)
	if block == nil || block.Type != "CERTIFICATE" {
		t.Fatal("LoadCACert returned invalid PEM")
	}
}

func TestGenerateCA_KeyPermissions(t *testing.T) {
	mgr := &CAManager{BasePath: t.TempDir()}
	if err := mgr.GenerateCA("perm-test"); err != nil {
		t.Fatalf("GenerateCA: %v", err)
	}

	info, err := os.Stat(filepath.Join(mgr.BasePath, "perm-test", "ca.key"))
	if err != nil {
		t.Fatalf("stat ca.key: %v", err)
	}
	perm := info.Mode().Perm()
	if perm != 0600 {
		t.Errorf("ca.key permissions = %o, want 0600", perm)
	}
}

func TestSignNodeCert_LocahostSAN(t *testing.T) {
	mgr := &CAManager{BasePath: t.TempDir()}
	if err := mgr.GenerateCA("test-cluster"); err != nil {
		t.Fatalf("GenerateCA: %v", err)
	}

	certPEM, _, err := mgr.SignNodeCert("test-cluster", "n1", "10.0.0.5")
	if err != nil {
		t.Fatalf("SignNodeCert: %v", err)
	}

	cert := parsePEMCert(t, certPEM)
	hasLocalhost := false
	for _, ip := range cert.IPAddresses {
		if ip.String() == "127.0.0.1" {
			hasLocalhost = true
		}
	}
	if !hasLocalhost {
		t.Error("node cert missing localhost (127.0.0.1) IP SAN")
	}

	hasDNS := false
	for _, dns := range cert.DNSNames {
		if dns == "localhost" {
			hasDNS = true
		}
	}
	if !hasDNS {
		t.Error("node cert missing localhost DNS SAN")
	}
}

// -- helpers --

func assertFileExists(t *testing.T, path string) {
	t.Helper()
	if _, err := os.Stat(path); err != nil {
		t.Errorf("expected file %s to exist: %v", path, err)
	}
}

func loadTestCert(t *testing.T, path string) *x509.Certificate {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read cert %s: %v", path, err)
	}
	return parsePEMCert(t, data)
}

func parsePEMCert(t *testing.T, data []byte) *x509.Certificate {
	t.Helper()
	block, _ := pem.Decode(data)
	if block == nil {
		t.Fatal("no PEM block in cert data")
	}
	cert, err := x509.ParseCertificate(block.Bytes)
	if err != nil {
		t.Fatalf("parse cert: %v", err)
	}
	return cert
}

func loadTestKey(t *testing.T, path string) *ecdsa.PrivateKey {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read key %s: %v", path, err)
	}
	block, _ := pem.Decode(data)
	if block == nil {
		t.Fatal("no PEM block in key data")
	}
	key, err := x509.ParseECPrivateKey(block.Bytes)
	if err != nil {
		t.Fatalf("parse key: %v", err)
	}
	return key
}
