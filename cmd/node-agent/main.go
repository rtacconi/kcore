package main

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"flag"
	"log"
	"net"
	"os"
	"os/signal"
	"syscall"

	nodeapi "github.com/kcore/kcore/api/node"
	"github.com/kcore/kcore/node"
	libvirtmgr "github.com/kcore/kcore/node/libvirt"
	"github.com/kcore/kcore/node/storage"
	"github.com/kcore/kcore/pkg/config"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
)

func main() {
	configPath := flag.String("config", "/etc/kcode/node-agent.yaml", "Path to node agent configuration file")
	flag.Parse()

	// Load configuration
	var cfg *config.NodeAgentConfig
	var err error
	if _, err := os.Stat(*configPath); err == nil {
		cfg, err = config.LoadNodeAgentConfig(*configPath)
		if err != nil {
			log.Fatalf("Failed to load config: %v", err)
		}
	} else {
		log.Printf("Config file not found, using defaults")
		cfg = config.DefaultNodeAgentConfig()
	}

	if cfg.NodeID == "" {
		hostname, _ := os.Hostname()
		cfg.NodeID = hostname
	}

	// Initialize libvirt
	libvirtMgr, err := libvirtmgr.New()
	if err != nil {
		log.Fatalf("Failed to initialize libvirt: %v", err)
	}
	defer libvirtMgr.Close()

	// Initialize storage drivers
	storageReg := storage.NewDriverRegistry()

	// Register storage drivers from config
	for driverName, driverCfg := range cfg.Storage.Drivers {
		switch driverCfg.Type {
		case "local-dir":
			path := driverCfg.Parameters["path"]
			if path == "" {
				path = "/var/lib/kcode/disks"
			}
			driver, err := storage.NewLocalDirDriver(path)
			if err != nil {
				log.Fatalf("Failed to create local-dir driver: %v", err)
			}
			storageReg.Register(driver)
			log.Printf("Registered storage driver: %s (local-dir at %s)", driverName, path)

		case "local-lvm":
			vg := driverCfg.Parameters["volumeGroup"]
			if vg == "" {
				log.Fatalf("volumeGroup parameter required for local-lvm driver")
			}
			driver, err := storage.NewLocalLVMDriver(vg)
			if err != nil {
				log.Fatalf("Failed to create local-lvm driver: %v", err)
			}
			storageReg.Register(driver)
			log.Printf("Registered storage driver: %s (local-lvm on VG %s)", driverName, vg)

		default:
			log.Printf("Warning: unknown storage driver type: %s", driverCfg.Type)
		}
	}

	// Create gRPC server
	server := grpc.NewServer(grpc.Creds(loadTLSCredentials(cfg.TLS)))

	// Create node server
	nodeServer, err := node.NewServer(cfg.NodeID, libvirtMgr, storageReg, cfg.Networks)
	if err != nil {
		log.Fatalf("Failed to create node server: %v", err)
	}

	// Register services
	nodeapi.RegisterNodeComputeServer(server, nodeServer)
	nodeapi.RegisterNodeStorageServer(server, nodeServer)
	nodeapi.RegisterNodeInfoServer(server, nodeServer)

	// Start listening
	listenAddr := ":9091" // Default node agent port
	lis, err := net.Listen("tcp", listenAddr)
	if err != nil {
		log.Fatalf("Failed to listen: %v", err)
	}

	_, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Handle signals
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		<-sigChan
		log.Println("Shutting down...")
		server.GracefulStop()
		cancel()
	}()

	log.Printf("Starting kcore node agent (node ID: %s)", cfg.NodeID)
	log.Printf("Listening on %s", listenAddr)

	if err := server.Serve(lis); err != nil {
		log.Fatalf("Failed to serve: %v", err)
	}
}

func loadTLSCredentials(tlsCfg config.TLSConfig) credentials.TransportCredentials {
	// Load CA certificate
	caCert, err := os.ReadFile(tlsCfg.CAFile)
	if err != nil {
		log.Fatalf("Failed to load CA certificate: %v", err)
	}

	caCertPool := x509.NewCertPool()
	if !caCertPool.AppendCertsFromPEM(caCert) {
		log.Fatalf("Failed to parse CA certificate")
	}

	// Load server certificate and key
	cert, err := tls.LoadX509KeyPair(tlsCfg.CertFile, tlsCfg.KeyFile)
	if err != nil {
		log.Fatalf("Failed to load server certificate: %v", err)
	}

	config := &tls.Config{
		Certificates: []tls.Certificate{cert},
		ClientAuth:   tls.RequireAndVerifyClientCert,
		ClientCAs:    caCertPool,
		MinVersion:   tls.VersionTLS12,
	}

	return credentials.NewTLS(config)
}
