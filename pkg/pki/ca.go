package pki

import (
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/pem"
	"fmt"
	"math/big"
	"net"
	"os"
	"path/filepath"
	"time"
)

const (
	caValidityYears   = 10
	certValidityYears = 5
)

// CAManager manages a kcore cluster CA and signs certificates.
// BasePath is injectable for testing (use temp dirs in tests).
type CAManager struct {
	BasePath string
}

// NewCAManager returns a CAManager rooted at basePath.
// If basePath is empty it defaults to ~/.kcore/clusters.
func NewCAManager(basePath string) (*CAManager, error) {
	if basePath == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return nil, fmt.Errorf("cannot determine home directory: %w", err)
		}
		basePath = filepath.Join(home, ".kcore", "clusters")
	}
	return &CAManager{BasePath: basePath}, nil
}

func (m *CAManager) clusterDir(clusterName string) string {
	return filepath.Join(m.BasePath, clusterName)
}

// GenerateCA creates a self-signed CA key + certificate for the given cluster.
func (m *CAManager) GenerateCA(clusterName string) error {
	dir := m.clusterDir(clusterName)
	if err := os.MkdirAll(dir, 0700); err != nil {
		return fmt.Errorf("create cluster dir: %w", err)
	}

	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		return fmt.Errorf("generate CA key: %w", err)
	}

	serial, err := randSerial()
	if err != nil {
		return err
	}

	now := time.Now()
	tmpl := &x509.Certificate{
		SerialNumber: serial,
		Subject: pkix.Name{
			CommonName:   "kcore-ca",
			Organization: []string{"kcore"},
		},
		NotBefore:             now,
		NotAfter:              now.AddDate(caValidityYears, 0, 0),
		KeyUsage:              x509.KeyUsageCertSign | x509.KeyUsageCRLSign,
		BasicConstraintsValid: true,
		IsCA:                  true,
		MaxPathLen:            1,
	}

	certDER, err := x509.CreateCertificate(rand.Reader, tmpl, tmpl, &key.PublicKey, key)
	if err != nil {
		return fmt.Errorf("create CA certificate: %w", err)
	}

	if err := writeKeyFile(filepath.Join(dir, "ca.key"), key); err != nil {
		return err
	}
	return writeCertFile(filepath.Join(dir, "ca.crt"), certDER)
}

// SignNodeCert generates a TLS certificate for a node, signed by the cluster CA.
func (m *CAManager) SignNodeCert(clusterName, nodeID, ipAddr string) (certPEM, keyPEM []byte, err error) {
	return m.signCert(clusterName, "kcore-node-"+nodeID, ipAddr, true, true)
}

// SignControllerCert generates a TLS certificate for a controller, signed by the cluster CA.
func (m *CAManager) SignControllerCert(clusterName, ipAddr string) (certPEM, keyPEM []byte, err error) {
	return m.signCert(clusterName, "kcore-controller", ipAddr, true, true)
}

// LoadCACert returns the PEM-encoded CA certificate bytes for the cluster.
func (m *CAManager) LoadCACert(clusterName string) ([]byte, error) {
	return os.ReadFile(filepath.Join(m.clusterDir(clusterName), "ca.crt"))
}

// WriteNodeCerts writes signed node cert+key+CA to a directory (e.g., for pushing to a node).
func (m *CAManager) WriteNodeCerts(clusterName, nodeID, ipAddr, outDir string) error {
	certPEM, keyPEM, err := m.SignNodeCert(clusterName, nodeID, ipAddr)
	if err != nil {
		return err
	}
	caPEM, err := m.LoadCACert(clusterName)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(outDir, 0700); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(outDir, "node.crt"), certPEM, 0644); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(outDir, "node.key"), keyPEM, 0600); err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(outDir, "ca.crt"), caPEM, 0644)
}

// WriteControllerCerts writes signed controller cert+key+CA to a directory.
func (m *CAManager) WriteControllerCerts(clusterName, ipAddr, outDir string) error {
	certPEM, keyPEM, err := m.SignControllerCert(clusterName, ipAddr)
	if err != nil {
		return err
	}
	caPEM, err := m.LoadCACert(clusterName)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(outDir, 0700); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(outDir, "controller.crt"), certPEM, 0644); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(outDir, "controller.key"), keyPEM, 0600); err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(outDir, "ca.crt"), caPEM, 0644)
}

// ClusterExists returns true if the cluster directory and CA cert already exist.
func (m *CAManager) ClusterExists(clusterName string) bool {
	_, err := os.Stat(filepath.Join(m.clusterDir(clusterName), "ca.crt"))
	return err == nil
}

func (m *CAManager) signCert(clusterName, cn, ipAddr string, serverAuth, clientAuth bool) ([]byte, []byte, error) {
	dir := m.clusterDir(clusterName)

	caCert, caKey, err := m.loadCA(dir)
	if err != nil {
		return nil, nil, fmt.Errorf("load CA: %w", err)
	}

	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		return nil, nil, fmt.Errorf("generate key: %w", err)
	}

	serial, err := randSerial()
	if err != nil {
		return nil, nil, err
	}

	now := time.Now()
	tmpl := &x509.Certificate{
		SerialNumber: serial,
		Subject: pkix.Name{
			CommonName:   cn,
			Organization: []string{"kcore"},
		},
		NotBefore: now,
		NotAfter:  now.AddDate(certValidityYears, 0, 0),
		KeyUsage:  x509.KeyUsageDigitalSignature | x509.KeyUsageKeyEncipherment,
		DNSNames:  []string{cn, "localhost"},
	}

	if serverAuth {
		tmpl.ExtKeyUsage = append(tmpl.ExtKeyUsage, x509.ExtKeyUsageServerAuth)
	}
	if clientAuth {
		tmpl.ExtKeyUsage = append(tmpl.ExtKeyUsage, x509.ExtKeyUsageClientAuth)
	}

	if ip := net.ParseIP(ipAddr); ip != nil {
		tmpl.IPAddresses = append(tmpl.IPAddresses, ip)
	}
	tmpl.IPAddresses = append(tmpl.IPAddresses, net.IPv4(127, 0, 0, 1))

	certDER, err := x509.CreateCertificate(rand.Reader, tmpl, caCert, &key.PublicKey, caKey)
	if err != nil {
		return nil, nil, fmt.Errorf("sign certificate: %w", err)
	}

	certPEM := pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: certDER})

	keyDER, err := x509.MarshalECPrivateKey(key)
	if err != nil {
		return nil, nil, fmt.Errorf("marshal key: %w", err)
	}
	keyPEM := pem.EncodeToMemory(&pem.Block{Type: "EC PRIVATE KEY", Bytes: keyDER})

	return certPEM, keyPEM, nil
}

func (m *CAManager) loadCA(dir string) (*x509.Certificate, *ecdsa.PrivateKey, error) {
	certPEM, err := os.ReadFile(filepath.Join(dir, "ca.crt"))
	if err != nil {
		return nil, nil, fmt.Errorf("read CA cert: %w", err)
	}
	block, _ := pem.Decode(certPEM)
	if block == nil {
		return nil, nil, fmt.Errorf("no PEM block in CA cert")
	}
	cert, err := x509.ParseCertificate(block.Bytes)
	if err != nil {
		return nil, nil, fmt.Errorf("parse CA cert: %w", err)
	}

	keyPEM, err := os.ReadFile(filepath.Join(dir, "ca.key"))
	if err != nil {
		return nil, nil, fmt.Errorf("read CA key: %w", err)
	}
	keyBlock, _ := pem.Decode(keyPEM)
	if keyBlock == nil {
		return nil, nil, fmt.Errorf("no PEM block in CA key")
	}
	key, err := x509.ParseECPrivateKey(keyBlock.Bytes)
	if err != nil {
		return nil, nil, fmt.Errorf("parse CA key: %w", err)
	}

	return cert, key, nil
}

func writeKeyFile(path string, key *ecdsa.PrivateKey) error {
	der, err := x509.MarshalECPrivateKey(key)
	if err != nil {
		return fmt.Errorf("marshal key: %w", err)
	}
	data := pem.EncodeToMemory(&pem.Block{Type: "EC PRIVATE KEY", Bytes: der})
	return os.WriteFile(path, data, 0600)
}

func writeCertFile(path string, certDER []byte) error {
	data := pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: certDER})
	return os.WriteFile(path, data, 0644)
}

func randSerial() (*big.Int, error) {
	serial, err := rand.Int(rand.Reader, new(big.Int).Lsh(big.NewInt(1), 128))
	if err != nil {
		return nil, fmt.Errorf("generate serial: %w", err)
	}
	return serial, nil
}
