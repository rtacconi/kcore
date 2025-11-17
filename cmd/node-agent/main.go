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
	"time"

	ctrlpb "github.com/kcore/kcore/api/controller"
	nodepb "github.com/kcore/kcore/api/node"
	"github.com/kcore/kcore/node"
	libvirtmgr "github.com/kcore/kcore/node/libvirt"
	"github.com/kcore/kcore/node/storage"
	"github.com/kcore/kcore/pkg/config"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
)

func main() {
	configPath := flag.String("config", "/etc/kcore/node-agent.yaml", "Path to node agent configuration file")
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
				path = "/var/lib/kcore/disks"
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
	nodepb.RegisterNodeComputeServer(server, nodeServer)
	nodepb.RegisterNodeStorageServer(server, nodeServer)
	nodepb.RegisterNodeInfoServer(server, nodeServer)

	// Start listening
	listenAddr := cfg.ListenAddr
	if listenAddr == "" {
		listenAddr = ":9091" // Fallback default
	}
	lis, err := net.Listen("tcp", listenAddr)
	if err != nil {
		log.Fatalf("Failed to listen: %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Start periodic state sync with controller if configured
	if cfg.ControllerAddr != "" {
		go startStateSyncLoop(ctx, cfg, nodeServer)
	}

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
	if cfg.ControllerAddr != "" {
		log.Printf("Will sync state to controller at %s every 10 seconds", cfg.ControllerAddr)
	}

	if err := server.Serve(lis); err != nil {
		log.Fatalf("Failed to serve: %v", err)
	}
}

func startStateSyncLoop(ctx context.Context, cfg *config.NodeAgentConfig, nodeServer *node.Server) {
	// Load TLS credentials for connecting to controller
	caCert, err := os.ReadFile(cfg.TLS.CAFile)
	if err != nil {
		log.Printf("Failed to load CA cert for controller connection: %v", err)
		return
	}

	caCertPool := x509.NewCertPool()
	if !caCertPool.AppendCertsFromPEM(caCert) {
		log.Printf("Failed to parse CA cert")
		return
	}

	cert, err := tls.LoadX509KeyPair(cfg.TLS.CertFile, cfg.TLS.KeyFile)
	if err != nil {
		log.Printf("Failed to load client cert for controller: %v", err)
		return
	}

	tlsConfig := &tls.Config{
		Certificates:       []tls.Certificate{cert},
		RootCAs:            caCertPool,
		InsecureSkipVerify: true, // Skip verification (IPs not in cert SANs)
	}

	// Connect to controller
	conn, err := grpc.Dial(cfg.ControllerAddr, grpc.WithTransportCredentials(credentials.NewTLS(tlsConfig)))
	if err != nil {
		log.Printf("Failed to connect to controller at %s: %v", cfg.ControllerAddr, err)
		return
	}
	defer conn.Close()

	client := ctrlpb.NewControllerClient(conn)
	log.Printf("Connected to controller at %s", cfg.ControllerAddr)

	// Get node's own address (required for registration)
	// Try to get from environment, config, or use hostname
	nodeAddress := os.Getenv("KCORE_NODE_ADDRESS")
	if nodeAddress == "" {
		// Could also try to auto-detect from network interfaces
		// For now, require explicit configuration
		hostname, _ := os.Hostname()
		nodeAddress = hostname + ":9091"
		log.Printf("Warning: KCORE_NODE_ADDRESS not set, using %s", nodeAddress)
	}

	// Register with controller first
	_, err = client.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId:   cfg.NodeID,
		Hostname: cfg.NodeID,
		Address:  nodeAddress,
		Capacity: &ctrlpb.NodeCapacity{
			CpuCores:    8,                       // TODO: Get from system info
			MemoryBytes: 16 * 1024 * 1024 * 1024, // 16GB TODO: Get from system info
		},
	})
	if err != nil {
		log.Printf("Failed to register with controller: %v", err)
		return
	}
	log.Printf("✅ Registered with controller")

	// Periodic sync loop
	ticker := time.NewTicker(10 * time.Second)
	defer ticker.Stop()

	// Do initial sync immediately
	syncState(ctx, client, cfg.NodeID, nodeServer)

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			syncState(ctx, client, cfg.NodeID, nodeServer)
		}
	}
}

func syncState(ctx context.Context, client ctrlpb.ControllerClient, nodeID string, nodeServer *node.Server) {
	// List all VMs from local libvirt
	resp, err := nodeServer.ListVms(ctx, &nodepb.ListVmsRequest{})
	if err != nil {
		log.Printf("Failed to list VMs for sync: %v", err)
		return
	}

	// Convert to controller VmInfo format
	var vms []*ctrlpb.VmInfo
	for _, vm := range resp.Vms {
		vms = append(vms, &ctrlpb.VmInfo{
			Id:          vm.Id,
			Name:        vm.Name,
			State:       ctrlpb.VmState(vm.State),
			Cpu:         vm.Cpu,
			MemoryBytes: vm.MemoryBytes,
			NodeId:      nodeID,
		})
	}

	// Send state to controller
	_, err = client.SyncVmState(ctx, &ctrlpb.SyncVmStateRequest{
		NodeId: nodeID,
		Vms:    vms,
	})
	if err != nil {
		log.Printf("Failed to sync state to controller: %v", err)
		return
	}

	log.Printf("Synced state: %d VMs to controller", len(vms))
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
