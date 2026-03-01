package controlplaneclient

import (
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"os"

	cppb "github.com/kcore/kcore/api/controlplane"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
)

type Config struct {
	Address    string
	CertFile   string
	KeyFile    string
	CAFile     string
	Insecure   bool // Skip server cert verification only for development.
	ServerName string
}

func Dial(cfg Config) (*grpc.ClientConn, cppb.ControlPlaneClient, error) {
	if cfg.Address == "" {
		return nil, nil, fmt.Errorf("address is required")
	}

	caPEM, err := os.ReadFile(cfg.CAFile)
	if err != nil {
		return nil, nil, fmt.Errorf("read CA file: %w", err)
	}

	cert, err := tls.LoadX509KeyPair(cfg.CertFile, cfg.KeyFile)
	if err != nil {
		return nil, nil, fmt.Errorf("load client certificate: %w", err)
	}

	certPool := x509.NewCertPool()
	if !certPool.AppendCertsFromPEM(caPEM) {
		return nil, nil, fmt.Errorf("failed to parse CA bundle")
	}

	tlsConfig := &tls.Config{
		Certificates:       []tls.Certificate{cert},
		RootCAs:            certPool,
		InsecureSkipVerify: cfg.Insecure,
		ServerName:         cfg.ServerName,
		MinVersion:         tls.VersionTLS12,
	}

	conn, err := grpc.Dial(cfg.Address, grpc.WithTransportCredentials(credentials.NewTLS(tlsConfig)))
	if err != nil {
		return nil, nil, fmt.Errorf("dial controlplane: %w", err)
	}

	return conn, cppb.NewControlPlaneClient(conn), nil
}
