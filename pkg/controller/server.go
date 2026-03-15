package controller

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"os"
	"os/exec"
	"strings"
	"sync"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/credentials"
	"google.golang.org/grpc/status"
	"google.golang.org/protobuf/types/known/timestamppb"

	ctrlpb "github.com/kcore/kcore/api/controller"
	nodepb "github.com/kcore/kcore/api/node"
	"github.com/kcore/kcore/pkg/sqlite"
)

// Server implements the Controller service.
// When db is non-nil, node registrations and VM records are persisted to SQLite.
// When db is nil, everything is in-memory only (used in unit tests).
type Server struct {
	ctrlpb.UnimplementedControllerServer
	ctrlpb.UnimplementedControllerAdminServer
	db            *sqlite.DB
	nodes         map[string]*NodeInfo // nodeID -> NodeInfo (always in-memory for gRPC conns)
	vmToNode      map[string]string    // vmID -> nodeID (fallback when db is nil)
	nodeDialCreds credentials.TransportCredentials
	mu            sync.RWMutex
}

type NodeInfo struct {
	ID            string
	Hostname      string
	Address       string
	Capacity      *ctrlpb.NodeCapacity
	Usage         *ctrlpb.NodeUsage
	LastHeartbeat time.Time
	Status        string
	Client        nodepb.NodeComputeClient
	Conn          *grpc.ClientConn
}

// NewServer creates an in-memory-only server (used by tests).
func NewServer() *Server {
	return &Server{
		nodes:    make(map[string]*NodeInfo),
		vmToNode: make(map[string]string),
	}
}

// NewServerWithDB creates a server backed by SQLite for persistence.
func NewServerWithDB(db *sqlite.DB) *Server {
	return &Server{
		db:       db,
		nodes:    make(map[string]*NodeInfo),
		vmToNode: make(map[string]string),
	}
}

func (s *Server) SetNodeDialCredentials(creds credentials.TransportCredentials) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.nodeDialCreds = creds
}

// ControllerAdmin implementation

func (s *Server) ApplyNixConfig(ctx context.Context, req *ctrlpb.ApplyNixConfigRequest) (*ctrlpb.ApplyNixConfigResponse, error) {
	if req.GetConfigurationNix() == "" {
		return &ctrlpb.ApplyNixConfigResponse{
			Success: false,
			Message: "configuration_nix is empty",
		}, nil
	}

	const cfgPath = "/etc/nixos/configuration.nix"

	if err := os.MkdirAll("/etc/nixos", 0755); err != nil {
		return &ctrlpb.ApplyNixConfigResponse{
			Success: false,
			Message: fmt.Sprintf("failed to create /etc/nixos: %v", err),
		}, nil
	}

	if err := os.WriteFile(cfgPath, []byte(req.GetConfigurationNix()), 0644); err != nil {
		return &ctrlpb.ApplyNixConfigResponse{
			Success: false,
			Message: fmt.Sprintf("failed to write %s: %v", cfgPath, err),
		}, nil
	}

	if req.GetRebuild() {
		cmd := exec.CommandContext(ctx, "nixos-rebuild", "switch")
		output, err := cmd.CombinedOutput()
		if err != nil {
			return &ctrlpb.ApplyNixConfigResponse{
				Success: false,
				Message: fmt.Sprintf("nixos-rebuild switch failed: %v (output: %s)", err, strings.TrimSpace(string(output))),
			}, nil
		}
	}

	return &ctrlpb.ApplyNixConfigResponse{
		Success: true,
		Message: "configuration applied",
	}, nil
}

// Node Registration

func (s *Server) RegisterNode(ctx context.Context, req *ctrlpb.RegisterNodeRequest) (*ctrlpb.RegisterNodeResponse, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	var (
		conn         *grpc.ClientConn
		client       nodepb.NodeComputeClient
		nodeStatus   = "ready"
		responseMsg  = "Node registered successfully"
		dialWarnText string
	)
	if s.nodeDialCreds != nil && req.Address != "" {
		dialConn, err := grpc.Dial(req.Address, grpc.WithTransportCredentials(s.nodeDialCreds))
		if err != nil {
			nodeStatus = "degraded"
			dialWarnText = fmt.Sprintf(" (initial node dial failed: %v)", err)
			responseMsg = "Node registered, but initial controller-to-node connection failed"
		} else {
			conn = dialConn
			client = nodepb.NewNodeComputeClient(dialConn)
		}
	}

	nodeInfo := &NodeInfo{
		ID:            req.NodeId,
		Hostname:      req.Hostname,
		Address:       req.Address,
		Capacity:      req.Capacity,
		Usage:         &ctrlpb.NodeUsage{},
		LastHeartbeat: time.Now(),
		Status:        nodeStatus,
		Client:        client,
		Conn:          conn,
	}

	s.nodes[req.NodeId] = nodeInfo

	// Persist to SQLite
	if s.db != nil {
		dbNode := &sqlite.Node{
			ID:          req.NodeId,
			Hostname:    req.Hostname,
			Address:     req.Address,
			CPUCores:    int(req.Capacity.GetCpuCores()),
			MemoryBytes: req.Capacity.GetMemoryBytes(),
			Labels:      req.Labels,
		}
		if err := s.db.UpsertNode(dbNode); err != nil {
			log.Printf("Warning: failed to persist node %s to SQLite: %v", req.NodeId, err)
		}
	}

	log.Printf("Node registered: %s (%s) at %s%s", req.NodeId, req.Hostname, req.Address, dialWarnText)

	return &ctrlpb.RegisterNodeResponse{
		Success: true,
		Message: responseMsg,
	}, nil
}

func (s *Server) Heartbeat(ctx context.Context, req *ctrlpb.HeartbeatRequest) (*ctrlpb.HeartbeatResponse, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	node, exists := s.nodes[req.NodeId]
	if !exists {
		return nil, status.Errorf(codes.NotFound, "node not found: %s", req.NodeId)
	}

	node.Usage = req.Usage
	node.LastHeartbeat = time.Now()
	node.Status = "ready"

	if s.db != nil {
		s.db.UpdateNodeHeartbeat(req.NodeId)
	}

	return &ctrlpb.HeartbeatResponse{
		Success: true,
	}, nil
}

func (s *Server) SyncVmState(ctx context.Context, req *ctrlpb.SyncVmStateRequest) (*ctrlpb.SyncVmStateResponse, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	_, exists := s.nodes[req.NodeId]
	if !exists {
		return nil, status.Errorf(codes.NotFound, "node not found: %s", req.NodeId)
	}

	currentVMIDs := make(map[string]bool)
	for _, vm := range req.Vms {
		currentVMIDs[vm.Id] = true
	}

	for vmID, nodeID := range s.vmToNode {
		if nodeID == req.NodeId && !currentVMIDs[vmID] {
			log.Printf("VM %s removed from node %s (no longer exists in libvirt)", vmID, req.NodeId)
			delete(s.vmToNode, vmID)
		}
	}

	for _, vm := range req.Vms {
		existingNode, exists := s.vmToNode[vm.Id]
		if !exists {
			log.Printf("VM %s discovered on node %s", vm.Id, req.NodeId)
			s.vmToNode[vm.Id] = req.NodeId
		} else if existingNode != req.NodeId {
			log.Printf("VM %s moved from node %s to %s", vm.Id, existingNode, req.NodeId)
			s.vmToNode[vm.Id] = req.NodeId
		}

		// Update actual state in SQLite
		if s.db != nil {
			actualState := vmStateString(vm.State)
			s.db.UpdateVMState(vm.Id, actualState)
			nodeIDStr := req.NodeId
			s.db.UpdateVMPlacement(&sqlite.VMPlacement{
				VMID:         vm.Id,
				ActualNodeID: &nodeIDStr,
				ActualState:  actualState,
			})
		}
	}

	log.Printf("State sync from node %s: %d VMs", req.NodeId, len(req.Vms))
	return &ctrlpb.SyncVmStateResponse{
		Success: true,
	}, nil
}

// VM Operations

func (s *Server) CreateVm(ctx context.Context, req *ctrlpb.CreateVmRequest) (*ctrlpb.CreateVmResponse, error) {
	var targetNode *NodeInfo
	var err error

	if req.TargetNode != "" {
		targetNode, err = s.getNodeByAddress(req.TargetNode)
		if err != nil {
			return nil, status.Errorf(codes.NotFound, "target node not found: %s", req.TargetNode)
		}
	} else {
		targetNode = s.selectNode()
		if targetNode == nil {
			return nil, status.Error(codes.Unavailable, "no available nodes")
		}
	}

	// Convert controller VmSpec to node VmSpec, forwarding all fields
	nodeSpec := &nodepb.VmSpec{
		Id:               req.Spec.Id,
		Name:             req.Spec.Name,
		Cpu:              req.Spec.Cpu,
		MemoryBytes:      req.Spec.MemoryBytes,
		Disks:            convertDisks(req.Spec.Disks),
		Nics:             convertNics(req.Spec.Nics),
		EnableKcoreLogin: req.Spec.EnableKcoreLogin,
	}

	// Forward image_uri and cloud_init_user_data to node
	resp, err := targetNode.Client.CreateVm(ctx, &nodepb.CreateVmRequest{
		Spec:              nodeSpec,
		ImageUri:          req.ImageUri,
		CloudInitUserData: req.CloudInitUserData,
	})
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to create VM on node: %v", err)
	}

	// Track in-memory
	s.mu.Lock()
	s.vmToNode[req.Spec.Id] = targetNode.ID
	s.mu.Unlock()

	// Persist desired state to SQLite
	if s.db != nil {
		specJSON, _ := json.Marshal(map[string]interface{}{
			"name":               req.Spec.Name,
			"cpu":                req.Spec.Cpu,
			"memory_bytes":       req.Spec.MemoryBytes,
			"image_uri":          req.ImageUri,
			"cloud_init":         req.CloudInitUserData,
			"enable_kcore_login": req.Spec.EnableKcoreLogin,
		})
		nodeIDStr := targetNode.ID
		vm := &sqlite.VM{
			ID:           req.Spec.Id,
			Name:         req.Spec.Name,
			Namespace:    "default",
			CPU:          int(req.Spec.Cpu),
			MemoryBytes:  req.Spec.MemoryBytes,
			NodeID:       &nodeIDStr,
			State:        vmStateString(convertVmState(resp.Status.State)),
			DesiredSpec:  string(specJSON),
			DesiredState: "running",
			ImageURI:     req.ImageUri,
		}
		if err := s.db.CreateVM(vm); err != nil {
			log.Printf("Warning: failed to persist VM %s to SQLite: %v", req.Spec.Id, err)
		} else {
			s.db.UpdateVMPlacement(&sqlite.VMPlacement{
				VMID:          req.Spec.Id,
				DesiredNodeID: &nodeIDStr,
				ActualNodeID:  &nodeIDStr,
				DesiredState:  "running",
				ActualState:   vmStateString(convertVmState(resp.Status.State)),
			})
		}
	}

	log.Printf("VM created: %s on node %s", req.Spec.Name, targetNode.ID)

	return &ctrlpb.CreateVmResponse{
		VmId:   resp.Status.Id,
		NodeId: targetNode.ID,
		State:  convertVmState(resp.Status.State),
	}, nil
}

func (s *Server) DeleteVm(ctx context.Context, req *ctrlpb.DeleteVmRequest) (*ctrlpb.DeleteVmResponse, error) {
	var targetNode *NodeInfo
	var err error

	if req.TargetNode != "" {
		targetNode, err = s.getNodeByAddress(req.TargetNode)
		if err != nil {
			return nil, status.Errorf(codes.NotFound, "target node not found: %s", req.TargetNode)
		}
	} else {
		s.mu.RLock()
		nodeID, ok := s.vmToNode[req.VmId]
		s.mu.RUnlock()

		if !ok {
			return nil, status.Errorf(codes.NotFound, "VM location unknown: %s", req.VmId)
		}

		s.mu.RLock()
		targetNode = s.nodes[nodeID]
		s.mu.RUnlock()

		if targetNode == nil {
			return nil, status.Errorf(codes.NotFound, "node not found: %s", nodeID)
		}
	}

	_, err = targetNode.Client.DeleteVm(ctx, &nodepb.DeleteVmRequest{
		VmId: req.VmId,
	})
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to delete VM: %v", err)
	}

	s.mu.Lock()
	delete(s.vmToNode, req.VmId)
	s.mu.Unlock()

	// Mark desired state as deleted in SQLite
	if s.db != nil {
		s.db.UpdateVMDesiredState(req.VmId, "deleted")
		s.db.UpdateVMState(req.VmId, "deleted")
	}

	log.Printf("VM deleted: %s from node %s", req.VmId, targetNode.ID)

	return &ctrlpb.DeleteVmResponse{
		Success: true,
	}, nil
}

func (s *Server) StartVm(ctx context.Context, req *ctrlpb.StartVmRequest) (*ctrlpb.StartVmResponse, error) {
	targetNode, err := s.findNodeForVm(req.VmId, req.TargetNode)
	if err != nil {
		return nil, err
	}

	resp, err := targetNode.Client.StartVm(ctx, &nodepb.StartVmRequest{
		VmId: req.VmId,
	})
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to start VM: %v", err)
	}

	if s.db != nil {
		s.db.UpdateVMDesiredState(req.VmId, "running")
		s.db.UpdateVMState(req.VmId, "running")
	}

	return &ctrlpb.StartVmResponse{
		State: convertVmState(resp.Status.State),
	}, nil
}

func (s *Server) StopVm(ctx context.Context, req *ctrlpb.StopVmRequest) (*ctrlpb.StopVmResponse, error) {
	targetNode, err := s.findNodeForVm(req.VmId, req.TargetNode)
	if err != nil {
		return nil, err
	}

	resp, err := targetNode.Client.StopVm(ctx, &nodepb.StopVmRequest{
		VmId:  req.VmId,
		Force: req.Force,
	})
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to stop VM: %v", err)
	}

	if s.db != nil {
		s.db.UpdateVMDesiredState(req.VmId, "stopped")
		s.db.UpdateVMState(req.VmId, "stopped")
	}

	return &ctrlpb.StopVmResponse{
		State: convertVmState(resp.Status.State),
	}, nil
}

func (s *Server) GetVm(ctx context.Context, req *ctrlpb.GetVmRequest) (*ctrlpb.GetVmResponse, error) {
	targetNode, err := s.findNodeForVm(req.VmId, req.TargetNode)
	if err != nil {
		return nil, err
	}

	resp, err := targetNode.Client.GetVm(ctx, &nodepb.GetVmRequest{
		VmId: req.VmId,
	})
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to get VM: %v", err)
	}

	var spec *ctrlpb.VmSpec
	if resp.Spec != nil {
		spec = &ctrlpb.VmSpec{
			Id:          resp.Spec.Id,
			Name:        resp.Spec.Name,
			Cpu:         resp.Spec.Cpu,
			MemoryBytes: resp.Spec.MemoryBytes,
			Disks:       convertDisksFromNode(resp.Spec.Disks),
			Nics:        convertNicsFromNode(resp.Spec.Nics),
		}
	}

	return &ctrlpb.GetVmResponse{
		Spec:   spec,
		Status: convertVmStatus(resp.Status),
		NodeId: targetNode.ID,
	}, nil
}

func (s *Server) ListVms(ctx context.Context, req *ctrlpb.ListVmsRequest) (*ctrlpb.ListVmsResponse, error) {
	var vms []*ctrlpb.VmInfo

	if req.TargetNode != "" {
		targetNode, err := s.getNodeByAddress(req.TargetNode)
		if err != nil {
			return nil, status.Errorf(codes.NotFound, "target node not found: %s", req.TargetNode)
		}

		resp, err := targetNode.Client.ListVms(ctx, &nodepb.ListVmsRequest{})
		if err != nil {
			return nil, status.Errorf(codes.Internal, "failed to list VMs from node: %v", err)
		}

		for _, vm := range resp.Vms {
			vms = append(vms, &ctrlpb.VmInfo{
				Id:          vm.Id,
				Name:        vm.Name,
				State:       convertVmState(vm.State),
				Cpu:         vm.Cpu,
				MemoryBytes: vm.MemoryBytes,
				NodeId:      targetNode.ID,
				CreatedAt:   vm.CreatedAt,
			})
		}
	} else {
		s.mu.RLock()
		nodes := make([]*NodeInfo, 0, len(s.nodes))
		for _, node := range s.nodes {
			nodes = append(nodes, node)
		}
		s.mu.RUnlock()

		for _, node := range nodes {
			if node.Client == nil {
				continue
			}
			resp, err := node.Client.ListVms(ctx, &nodepb.ListVmsRequest{})
			if err != nil {
				log.Printf("Warning: failed to list VMs from node %s: %v", node.ID, err)
				continue
			}

			for _, vm := range resp.Vms {
				vms = append(vms, &ctrlpb.VmInfo{
					Id:          vm.Id,
					Name:        vm.Name,
					State:       convertVmState(vm.State),
					Cpu:         vm.Cpu,
					MemoryBytes: vm.MemoryBytes,
					NodeId:      node.ID,
					CreatedAt:   vm.CreatedAt,
				})
			}
		}
	}

	return &ctrlpb.ListVmsResponse{
		Vms: vms,
	}, nil
}

// Node Operations

func (s *Server) ListNodes(ctx context.Context, req *ctrlpb.ListNodesRequest) (*ctrlpb.ListNodesResponse, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	nodes := make([]*ctrlpb.NodeInfo, 0, len(s.nodes))
	for _, node := range s.nodes {
		nodes = append(nodes, &ctrlpb.NodeInfo{
			NodeId:        node.ID,
			Hostname:      node.Hostname,
			Address:       node.Address,
			Capacity:      node.Capacity,
			Usage:         node.Usage,
			Status:        node.Status,
			LastHeartbeat: timestamppb.New(node.LastHeartbeat),
		})
	}

	return &ctrlpb.ListNodesResponse{
		Nodes: nodes,
	}, nil
}

func (s *Server) GetNode(ctx context.Context, req *ctrlpb.GetNodeRequest) (*ctrlpb.GetNodeResponse, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	node, exists := s.nodes[req.NodeId]
	if !exists {
		return nil, status.Errorf(codes.NotFound, "node not found: %s", req.NodeId)
	}

	return &ctrlpb.GetNodeResponse{
		Node: &ctrlpb.NodeInfo{
			NodeId:        node.ID,
			Hostname:      node.Hostname,
			Address:       node.Address,
			Capacity:      node.Capacity,
			Usage:         node.Usage,
			Status:        node.Status,
			LastHeartbeat: timestamppb.New(node.LastHeartbeat),
		},
	}, nil
}

// Helper functions

func (s *Server) getNodeByAddress(address string) (*NodeInfo, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	for _, node := range s.nodes {
		if node.Address == address {
			return node, nil
		}
	}
	return nil, fmt.Errorf("node not found with address: %s", address)
}

func (s *Server) findNodeForVm(vmID, targetNodeAddr string) (*NodeInfo, error) {
	if targetNodeAddr != "" {
		return s.getNodeByAddress(targetNodeAddr)
	}

	s.mu.RLock()
	nodeID, ok := s.vmToNode[vmID]
	s.mu.RUnlock()

	if !ok {
		return nil, status.Errorf(codes.NotFound, "VM location unknown: %s", vmID)
	}

	s.mu.RLock()
	node := s.nodes[nodeID]
	s.mu.RUnlock()

	if node == nil {
		return nil, status.Errorf(codes.NotFound, "node not found: %s", nodeID)
	}

	return node, nil
}

func (s *Server) selectNode() *NodeInfo {
	s.mu.RLock()
	defer s.mu.RUnlock()

	for _, node := range s.nodes {
		if node.Status == "ready" {
			return node
		}
	}
	return nil
}

// Conversion functions

func convertDisks(disks []*ctrlpb.Disk) []*nodepb.Disk {
	result := make([]*nodepb.Disk, len(disks))
	for i, d := range disks {
		result[i] = &nodepb.Disk{
			Name:          d.Name,
			BackendHandle: d.BackendHandle,
			Bus:           d.Bus,
			Device:        d.Device,
		}
	}
	return result
}

func convertNics(nics []*ctrlpb.Nic) []*nodepb.Nic {
	result := make([]*nodepb.Nic, len(nics))
	for i, n := range nics {
		result[i] = &nodepb.Nic{
			Network:    n.Network,
			Model:      n.Model,
			MacAddress: n.MacAddress,
		}
	}
	return result
}

func convertDisksFromNode(disks []*nodepb.Disk) []*ctrlpb.Disk {
	result := make([]*ctrlpb.Disk, len(disks))
	for i, d := range disks {
		result[i] = &ctrlpb.Disk{
			Name:          d.Name,
			BackendHandle: d.BackendHandle,
			Bus:           d.Bus,
			Device:        d.Device,
		}
	}
	return result
}

func convertNicsFromNode(nics []*nodepb.Nic) []*ctrlpb.Nic {
	result := make([]*ctrlpb.Nic, len(nics))
	for i, n := range nics {
		result[i] = &ctrlpb.Nic{
			Network:    n.Network,
			Model:      n.Model,
			MacAddress: n.MacAddress,
		}
	}
	return result
}

func convertVmState(state nodepb.VmState) ctrlpb.VmState {
	switch state {
	case nodepb.VmState_VM_STATE_STOPPED:
		return ctrlpb.VmState_VM_STATE_STOPPED
	case nodepb.VmState_VM_STATE_RUNNING:
		return ctrlpb.VmState_VM_STATE_RUNNING
	case nodepb.VmState_VM_STATE_PAUSED:
		return ctrlpb.VmState_VM_STATE_PAUSED
	case nodepb.VmState_VM_STATE_ERROR:
		return ctrlpb.VmState_VM_STATE_ERROR
	default:
		return ctrlpb.VmState_VM_STATE_UNKNOWN
	}
}

func convertVmStatus(status *nodepb.VmStatus) *ctrlpb.VmStatus {
	if status == nil {
		return nil
	}
	return &ctrlpb.VmStatus{
		Id:        status.Id,
		State:     convertVmState(status.State),
		CreatedAt: status.CreatedAt,
		UpdatedAt: status.UpdatedAt,
	}
}

func vmStateString(state ctrlpb.VmState) string {
	switch state {
	case ctrlpb.VmState_VM_STATE_RUNNING:
		return "running"
	case ctrlpb.VmState_VM_STATE_STOPPED:
		return "stopped"
	case ctrlpb.VmState_VM_STATE_PAUSED:
		return "paused"
	case ctrlpb.VmState_VM_STATE_ERROR:
		return "error"
	default:
		return "unknown"
	}
}
