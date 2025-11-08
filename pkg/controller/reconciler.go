package controller

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"log"
	"net"
	"os"
	"time"

	"github.com/google/uuid"
	controllerapi "github.com/kcore/kcore/api/controller"
	nodeapi "github.com/kcore/kcore/api/node"
	"github.com/kcore/kcore/pkg/config"
	"github.com/kcore/kcore/pkg/sqlite"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
)

type Controller struct {
	controllerapi.UnimplementedControllerServer
	db     *sqlite.DB
	config *config.ControllerConfig
	nodes  map[string]*NodeClient // nodeID -> gRPC client
	server *grpc.Server
}

type NodeClient struct {
	ID      string
	Address string
	Compute nodeapi.NodeComputeClient
	Storage nodeapi.NodeStorageClient
	Info    nodeapi.NodeInfoClient
	conn    *grpc.ClientConn
}

func New(db *sqlite.DB, cfg *config.ControllerConfig) (*Controller, error) {
	return &Controller{
		db:     db,
		config: cfg,
		nodes:  make(map[string]*NodeClient),
	}, nil
}

func (c *Controller) getNodeClient(nodeID string) (*NodeClient, error) {
	if client, ok := c.nodes[nodeID]; ok {
		return client, nil
	}

	node, err := c.db.GetNode(nodeID)
	if err != nil {
		return nil, fmt.Errorf("failed to get node: %w", err)
	}

	// Create gRPC connection with mTLS
	creds, err := credentials.NewClientTLSFromFile(c.config.TLS.CAFile, "")
	if err != nil {
		return nil, fmt.Errorf("failed to load TLS credentials: %w", err)
	}

	conn, err := grpc.NewClient(node.Address, grpc.WithTransportCredentials(creds))
	if err != nil {
		return nil, fmt.Errorf("failed to connect to node: %w", err)
	}

	client := &NodeClient{
		ID:      node.ID,
		Address: node.Address,
		Compute: nodeapi.NewNodeComputeClient(conn),
		Storage: nodeapi.NewNodeStorageClient(conn),
		Info:    nodeapi.NewNodeInfoClient(conn),
		conn:    conn,
	}

	c.nodes[nodeID] = client
	return client, nil
}

// ApplyVM applies a VM spec from YAML
func (c *Controller) ApplyVM(ctx context.Context, spec *config.VMSpec) error {
	vmID := uuid.New().String()
	if spec.Metadata.Namespace == "" {
		spec.Metadata.Namespace = "default"
	}

	// Parse memory
	memoryBytes, err := config.ParseSizeBytes(spec.Spec.MemoryBytes)
	if err != nil {
		return fmt.Errorf("invalid memory size: %w", err)
	}

	// Find suitable node based on nodeSelector
	nodeID, err := c.selectNode(spec.Spec.NodeSelector)
	if err != nil {
		return fmt.Errorf("failed to select node: %w", err)
	}

	// Create VM record
	vm := &sqlite.VM{
		ID:          vmID,
		Name:        spec.Metadata.Name,
		Namespace:   spec.Metadata.Namespace,
		CPU:         spec.Spec.CPU,
		MemoryBytes: memoryBytes,
		NodeID:      &nodeID,
		State:       "pending",
	}

	if err := c.db.CreateVM(vm); err != nil {
		return fmt.Errorf("failed to create VM record: %w", err)
	}

	// Create volumes for disks
	for i, diskSpec := range spec.Spec.Disks {
		volumeID := uuid.New().String()
		sizeBytes, err := config.ParseSizeBytes(diskSpec.SizeBytes)
		if err != nil {
			return fmt.Errorf("invalid disk size for %s: %w", diskSpec.Name, err)
		}

		// Get storage class to determine if shared
		sc, err := c.db.GetStorageClass(diskSpec.StorageClassName)
		if err != nil {
			return fmt.Errorf("failed to get storage class %s: %w", diskSpec.StorageClassName, err)
		}

		volume := &sqlite.Volume{
			ID:           volumeID,
			Name:         fmt.Sprintf("%s-%s", spec.Metadata.Name, diskSpec.Name),
			Namespace:    spec.Metadata.Namespace,
			StorageClass: diskSpec.StorageClassName,
			SizeBytes:    sizeBytes,
			Shared:       sc.Shared,
		}

		if !sc.Shared {
			volume.NodeID = &nodeID
		}

		if err := c.db.CreateVolume(volume); err != nil {
			return fmt.Errorf("failed to create volume: %w", err)
		}

		// Link disk to VM
		device := fmt.Sprintf("vd%c", 'a'+byte(i))
		bus := diskSpec.Bus
		if bus == "" {
			bus = "virtio"
		}

		vmDisk := &sqlite.VMDisk{
			VMID:     vmID,
			DiskName: diskSpec.Name,
			VolumeID: volumeID,
			Bus:      bus,
			Device:   device,
		}

		if err := c.db.AddVMDisk(vmDisk); err != nil {
			return fmt.Errorf("failed to add VM disk: %w", err)
		}
	}

	// Add NICs
	for _, nicSpec := range spec.Spec.NICs {
		model := nicSpec.Model
		if model == "" {
			model = "virtio"
		}

		vmNIC := &sqlite.VMNIC{
			VMID:    vmID,
			Network: nicSpec.Network,
			Model:   model,
		}

		if err := c.db.AddVMNIC(vmNIC); err != nil {
			return fmt.Errorf("failed to add VM NIC: %w", err)
		}
	}

	// Set desired state
	placement := &sqlite.VMPlacement{
		VMID:          vmID,
		DesiredNodeID: &nodeID,
		DesiredState:  "stopped", // Start stopped by default
		ActualState:   "unknown",
	}

	if err := c.db.UpdateVMPlacement(placement); err != nil {
		return fmt.Errorf("failed to update VM placement: %w", err)
	}

	log.Printf("Created VM %s (%s) on node %s", spec.Metadata.Name, vmID, nodeID)
	return nil
}

func (c *Controller) selectNode(nodeSelector map[string]string) (string, error) {
	nodes, err := c.db.ListNodes()
	if err != nil {
		return "", err
	}

	if len(nodes) == 0 {
		return "", fmt.Errorf("no nodes available")
	}

	// Simple selection: first node that matches selector (or first node if no selector)
	// TODO: implement proper scheduling with capacity checks
	for _, node := range nodes {
		if len(nodeSelector) == 0 {
			return node.ID, nil
		}

		// Simple label matching (TODO: proper label matching)
		matches := true
		for k, v := range nodeSelector {
			// Check if node has this label
			found := false
			for _, label := range node.Labels {
				if label == fmt.Sprintf("%s=%s", k, v) {
					found = true
					break
				}
			}
			if !found {
				matches = false
				break
			}
		}

		if matches {
			return node.ID, nil
		}
	}

	// Fallback to first node
	return nodes[0].ID, nil
}

// Reconcile runs the reconciliation loop
func (c *Controller) Reconcile(ctx context.Context) error {
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
			if err := c.reconcileOnce(ctx); err != nil {
				log.Printf("Reconciliation error: %v", err)
			}
		}
	}
}

func (c *Controller) reconcileOnce(ctx context.Context) error {
	placements, err := c.db.ListVMsForReconciliation()
	if err != nil {
		return fmt.Errorf("failed to list VMs for reconciliation: %w", err)
	}

	for _, placement := range placements {
		if err := c.reconcileVM(ctx, placement); err != nil {
			log.Printf("Failed to reconcile VM %s: %v", placement.VMID, err)
			continue
		}
	}

	return nil
}

func (c *Controller) reconcileVM(ctx context.Context, placement *sqlite.VMPlacement) error {
	vm, err := c.db.GetVM(placement.VMID)
	if err != nil {
		return fmt.Errorf("failed to get VM: %w", err)
	}

	// Check if we need to move the VM to a different node
	if placement.DesiredNodeID != nil && placement.ActualNodeID != placement.DesiredNodeID {
		if placement.ActualNodeID != nil {
			// VM is running on wrong node - stop it first
			if err := c.stopVMOnNode(ctx, *placement.ActualNodeID, placement.VMID); err != nil {
				return fmt.Errorf("failed to stop VM on old node: %w", err)
			}
		}
		// Start on new node (will be handled below)
	}

	// Get node client
	if placement.DesiredNodeID == nil {
		return fmt.Errorf("no desired node for VM %s", placement.VMID)
	}

	client, err := c.getNodeClient(*placement.DesiredNodeID)
	if err != nil {
		return fmt.Errorf("failed to get node client: %w", err)
	}

	// Ensure volumes are provisioned and attached
	if err := c.ensureVolumes(ctx, client, vm); err != nil {
		return fmt.Errorf("failed to ensure volumes: %w", err)
	}

	// Handle state changes
	if placement.DesiredState != placement.ActualState {
		switch placement.DesiredState {
		case "running":
			if placement.ActualState == "stopped" || placement.ActualState == "unknown" {
				if err := c.startVM(ctx, client, vm); err != nil {
					return fmt.Errorf("failed to start VM: %w", err)
				}
				placement.ActualState = "running"
				placement.ActualNodeID = placement.DesiredNodeID
			}
		case "stopped":
			if placement.ActualState == "running" {
				if err := c.stopVMOnNode(ctx, *placement.DesiredNodeID, placement.VMID); err != nil {
					return fmt.Errorf("failed to stop VM: %w", err)
				}
				placement.ActualState = "stopped"
			}
		}
	}

	// Update placement
	if err := c.db.UpdateVMPlacement(placement); err != nil {
		return fmt.Errorf("failed to update placement: %w", err)
	}

	return nil
}

func (c *Controller) ensureVolumes(ctx context.Context, client *NodeClient, vm *sqlite.VM) error {
	disks, err := c.db.GetVMDisks(vm.ID)
	if err != nil {
		return err
	}

	for _, disk := range disks {
		volume, err := c.db.GetVolume(disk.VolumeID)
		if err != nil {
			return fmt.Errorf("failed to get volume %s: %w", disk.VolumeID, err)
		}

		// If volume not yet provisioned, create it
		if volume.BackendHandle == "" {
			sc, err := c.db.GetStorageClass(volume.StorageClass)
			if err != nil {
				return fmt.Errorf("failed to get storage class: %w", err)
			}

			req := &nodeapi.CreateVolumeRequest{
				VolumeId:     volume.ID,
				StorageClass: volume.StorageClass,
				SizeBytes:    volume.SizeBytes,
				Parameters:   sc.Parameters,
			}

			resp, err := client.Storage.CreateVolume(ctx, req)
			if err != nil {
				return fmt.Errorf("failed to create volume: %w", err)
			}

			if err := c.db.UpdateVolumeBackendHandle(volume.ID, resp.BackendHandle); err != nil {
				return fmt.Errorf("failed to update volume backend handle: %w", err)
			}

			volume.BackendHandle = resp.BackendHandle
		}

		// Attach volume to VM (if not already attached)
		// TODO: track attachment state
		attachReq := &nodeapi.AttachVolumeRequest{
			BackendHandle: volume.BackendHandle,
			VmId:          vm.ID,
			TargetDevice:  disk.Device,
			Bus:           disk.Bus,
		}

		if _, err := client.Storage.AttachVolume(ctx, attachReq); err != nil {
			// Ignore "already attached" errors
			log.Printf("Note: volume attach returned error (may already be attached): %v", err)
		}
	}

	return nil
}

func (c *Controller) startVM(ctx context.Context, client *NodeClient, vm *sqlite.VM) error {
	// Build VM spec for node
	disks, err := c.db.GetVMDisks(vm.ID)
	if err != nil {
		return err
	}

	nics, err := c.db.GetVMNICs(vm.ID)
	if err != nil {
		return err
	}

	// Get volume backend handles
	vmDisks := make([]*nodeapi.Disk, 0, len(disks))
	for _, disk := range disks {
		volume, err := c.db.GetVolume(disk.VolumeID)
		if err != nil {
			return err
		}

		vmDisks = append(vmDisks, &nodeapi.Disk{
			Name:          disk.DiskName,
			BackendHandle: volume.BackendHandle,
			Bus:           disk.Bus,
			Device:        disk.Device,
		})
	}

	vmNICs := make([]*nodeapi.Nic, 0, len(nics))
	for _, nic := range nics {
		vmNICs = append(vmNICs, &nodeapi.Nic{
			Network:    nic.Network,
			Model:      nic.Model,
			MacAddress: "",
		})
	}

	vmSpec := &nodeapi.VmSpec{
		Id:          vm.ID,
		Name:        vm.Name,
		Cpu:         int32(vm.CPU),
		MemoryBytes: vm.MemoryBytes,
		Disks:       vmDisks,
		Nics:        vmNICs,
	}

	// Check if VM exists
	getResp, err := client.Compute.GetVm(ctx, &nodeapi.GetVmRequest{VmId: vm.ID})
	if err != nil || getResp.Status.State == nodeapi.VmState_VM_STATE_UNKNOWN {
		// Create VM
		_, err = client.Compute.CreateVm(ctx, &nodeapi.CreateVmRequest{Spec: vmSpec})
		if err != nil {
			return fmt.Errorf("failed to create VM: %w", err)
		}
	} else {
		// Update VM
		_, err = client.Compute.UpdateVm(ctx, &nodeapi.UpdateVmRequest{Spec: vmSpec})
		if err != nil {
			return fmt.Errorf("failed to update VM: %w", err)
		}
	}

	// Start VM
	_, err = client.Compute.StartVm(ctx, &nodeapi.StartVmRequest{VmId: vm.ID})
	if err != nil {
		return fmt.Errorf("failed to start VM: %w", err)
	}

	if err := c.db.UpdateVMState(vm.ID, "running"); err != nil {
		return err
	}

	return nil
}

func (c *Controller) stopVMOnNode(ctx context.Context, nodeID, vmID string) error {
	client, err := c.getNodeClient(nodeID)
	if err != nil {
		return err
	}

	_, err = client.Compute.StopVm(ctx, &nodeapi.StopVmRequest{VmId: vmID, Force: false})
	if err != nil {
		return fmt.Errorf("failed to stop VM: %w", err)
	}

	if err := c.db.UpdateVMState(vmID, "stopped"); err != nil {
		return err
	}

	return nil
}

// RegisterNode handles node registration requests
func (c *Controller) RegisterNode(ctx context.Context, req *controllerapi.RegisterNodeRequest) (*controllerapi.RegisterNodeResponse, error) {
	log.Printf("Node registration request: ID=%s, Hostname=%s, Address=%s", req.NodeId, req.Hostname, req.Address)

	node := &sqlite.Node{
		ID:          req.NodeId,
		Hostname:    req.Hostname,
		Address:     req.Address,
		CPUCores:    int(req.Capacity.CpuCores),
		MemoryBytes: req.Capacity.MemoryBytes,
		Labels:      req.Labels,
	}

	if err := c.db.UpsertNode(node); err != nil {
		log.Printf("Failed to register node %s: %v", req.NodeId, err)
		return &controllerapi.RegisterNodeResponse{
			Success: false,
			Message: fmt.Sprintf("Failed to register: %v", err),
		}, nil
	}

	log.Printf("Successfully registered node %s (%s)", req.NodeId, req.Hostname)
	return &controllerapi.RegisterNodeResponse{
		Success: true,
		Message: "Node registered successfully",
	}, nil
}

// Heartbeat handles node heartbeat requests
func (c *Controller) Heartbeat(ctx context.Context, req *controllerapi.HeartbeatRequest) (*controllerapi.HeartbeatResponse, error) {
	// Update last heartbeat timestamp
	node := &sqlite.Node{
		ID: req.NodeId,
		// Other fields will be updated from existing record
	}

	// Get existing node to preserve fields
	existing, err := c.db.GetNode(req.NodeId)
	if err != nil {
		log.Printf("Heartbeat from unknown node %s: %v", req.NodeId, err)
		return &controllerapi.HeartbeatResponse{Success: false}, nil
	}

	// Update heartbeat timestamp
	node.Hostname = existing.Hostname
	node.Address = existing.Address
	node.CPUCores = existing.CPUCores
	node.MemoryBytes = existing.MemoryBytes
	node.Labels = existing.Labels

	if err := c.db.UpsertNode(node); err != nil {
		log.Printf("Failed to update heartbeat for node %s: %v", req.NodeId, err)
		return &controllerapi.HeartbeatResponse{Success: false}, nil
	}

	return &controllerapi.HeartbeatResponse{Success: true}, nil
}

// StartServer starts the gRPC server for node registration
func (c *Controller) StartServer(ctx context.Context) error {
	// Load CA certificate for client verification
	caCert, err := os.ReadFile(c.config.TLS.CAFile)
	if err != nil {
		return fmt.Errorf("failed to load CA certificate: %w", err)
	}

	caCertPool := x509.NewCertPool()
	if !caCertPool.AppendCertsFromPEM(caCert) {
		return fmt.Errorf("failed to parse CA certificate")
	}

	// Load server certificate and key
	cert, err := tls.LoadX509KeyPair(c.config.TLS.CertFile, c.config.TLS.KeyFile)
	if err != nil {
		return fmt.Errorf("failed to load server certificate: %w", err)
	}

	// Configure TLS with mTLS (require client certificates)
	tlsConfig := &tls.Config{
		Certificates: []tls.Certificate{cert},
		ClientAuth:   tls.RequireAndVerifyClientCert,
		ClientCAs:    caCertPool,
		MinVersion:   tls.VersionTLS12,
	}

	creds := credentials.NewTLS(tlsConfig)

	// Create gRPC server with mTLS
	c.server = grpc.NewServer(
		grpc.Creds(creds),
	)

	// Register controller service
	controllerapi.RegisterControllerServer(c.server, c)

	// Start listening
	lis, err := net.Listen("tcp", c.config.ListenAddr)
	if err != nil {
		return fmt.Errorf("failed to listen on %s: %w", c.config.ListenAddr, err)
	}

	log.Printf("gRPC server listening on %s", c.config.ListenAddr)

	// Start server in goroutine
	go func() {
		if err := c.server.Serve(lis); err != nil {
			log.Printf("gRPC server error: %v", err)
		}
	}()

	// Wait for context cancellation
	<-ctx.Done()
	log.Println("Shutting down gRPC server...")
	c.server.GracefulStop()

	return nil
}

func (c *Controller) Close() error {
	if c.server != nil {
		c.server.GracefulStop()
	}
	for _, client := range c.nodes {
		if client.conn != nil {
			client.conn.Close()
		}
	}
	return c.db.Close()
}
