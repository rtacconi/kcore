package node

import (
	"context"
	"fmt"
	"log"
	"os"
	"os/exec"
	"strings"

	"github.com/kcore/kcore/api/node"
	libvirtmgr "github.com/kcore/kcore/node/libvirt"
	"github.com/kcore/kcore/node/storage"
	libvirt "github.com/libvirt/libvirt-go"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
)

type Server struct {
	node.UnimplementedNodeComputeServer
	node.UnimplementedNodeStorageServer
	node.UnimplementedNodeInfoServer

	libvirtMgr *libvirtmgr.Manager
	storageReg *storage.DriverRegistry
	networks   map[string]string // network name -> bridge name
	nodeID     string
	hostname   string
}

func NewServer(nodeID string, libvirtMgr *libvirtmgr.Manager, storageReg *storage.DriverRegistry, networks map[string]string) (*Server, error) {
	hostname, err := os.Hostname()
	if err != nil {
		hostname = nodeID
	}

	return &Server{
		libvirtMgr: libvirtMgr,
		storageReg: storageReg,
		networks:   networks,
		nodeID:     nodeID,
		hostname:   hostname,
	}, nil
}

// NodeCompute implementation

func (s *Server) CreateVm(ctx context.Context, req *node.CreateVmRequest) (*node.CreateVmResponse, error) {
	spec := req.Spec
	if spec == nil {
		return nil, status.Error(codes.InvalidArgument, "vm spec is required")
	}

	// Convert protobuf spec to libvirt spec
	libvirtSpec := &libvirtmgr.VMSpec{
		ID:          spec.Id,
		Name:        spec.Name,
		CPU:         spec.Cpu,
		MemoryBytes: spec.MemoryBytes,
	}

	// Convert disks
	libvirtSpec.Disks = make([]libvirtmgr.DiskSpec, 0, len(spec.Disks))
	for _, disk := range spec.Disks {
		libvirtSpec.Disks = append(libvirtSpec.Disks, libvirtmgr.DiskSpec{
			Name:          disk.Name,
			BackendHandle: disk.BackendHandle,
			Bus:           disk.Bus,
			Device:        disk.Device,
		})
	}

	// Convert NICs - map network names to bridge names
	libvirtSpec.NICs = make([]libvirtmgr.NICSpec, 0, len(spec.Nics))
	for _, nic := range spec.Nics {
		bridgeName := s.networks[nic.Network]
		if bridgeName == "" {
			bridgeName = nic.Network // Fallback to network name
		}

		libvirtSpec.NICs = append(libvirtSpec.NICs, libvirtmgr.NICSpec{
			Network:    bridgeName,
			Model:      nic.Model,
			MACAddress: nic.MacAddress,
		})
	}

	// Build domain XML
	domainXML, err := libvirtmgr.BuildDomainXML(libvirtSpec)
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to build domain XML: %v", err)
	}

	xmlStr, err := domainXML.Marshal()
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to marshal domain XML: %v", err)
	}

	// Create domain
	_, err = s.libvirtMgr.CreateDomain(xmlStr)
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to create domain: %v", err)
	}

	log.Printf("Created VM %s (%s)", spec.Name, spec.Id)

	return &node.CreateVmResponse{
		Status: &node.VmStatus{
			Id:    spec.Id,
			State: node.VmState_VM_STATE_STOPPED,
		},
	}, nil
}

func (s *Server) UpdateVm(ctx context.Context, req *node.UpdateVmRequest) (*node.UpdateVmResponse, error) {
	spec := req.Spec
	if spec == nil {
		return nil, status.Error(codes.InvalidArgument, "vm spec is required")
	}

	// Build updated domain XML (similar to CreateVm)
	libvirtSpec := &libvirtmgr.VMSpec{
		ID:          spec.Id,
		Name:        spec.Name,
		CPU:         spec.Cpu,
		MemoryBytes: spec.MemoryBytes,
	}

	libvirtSpec.Disks = make([]libvirtmgr.DiskSpec, 0, len(spec.Disks))
	for _, disk := range spec.Disks {
		libvirtSpec.Disks = append(libvirtSpec.Disks, libvirtmgr.DiskSpec{
			Name:          disk.Name,
			BackendHandle: disk.BackendHandle,
			Bus:           disk.Bus,
			Device:        disk.Device,
		})
	}

	libvirtSpec.NICs = make([]libvirtmgr.NICSpec, 0, len(spec.Nics))
	for _, nic := range spec.Nics {
		bridgeName := s.networks[nic.Network]
		if bridgeName == "" {
			bridgeName = nic.Network
		}
		libvirtSpec.NICs = append(libvirtSpec.NICs, libvirtmgr.NICSpec{
			Network:    bridgeName,
			Model:      nic.Model,
			MACAddress: nic.MacAddress,
		})
	}

	domainXML, err := libvirtmgr.BuildDomainXML(libvirtSpec)
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to build domain XML: %v", err)
	}

	xmlStr, err := domainXML.Marshal()
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to marshal domain XML: %v", err)
	}

	// Update domain
	if err := s.libvirtMgr.UpdateDomain(xmlStr); err != nil {
		return nil, status.Errorf(codes.Internal, "failed to update domain: %v", err)
	}

	log.Printf("Updated VM %s (%s)", spec.Name, spec.Id)

	// Get current state
	state, err := s.libvirtMgr.GetDomainState(spec.Name)
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to get domain state: %v", err)
	}

	return &node.UpdateVmResponse{
		Status: &node.VmStatus{
			Id:    spec.Id,
			State: libvirtDomainStateToProto(state),
		},
	}, nil
}

func (s *Server) DeleteVm(ctx context.Context, req *node.DeleteVmRequest) (*node.DeleteVmResponse, error) {
	if req.VmId == "" {
		return nil, status.Error(codes.InvalidArgument, "vm_id is required")
	}

	// Get domain name from ID (for now, assume ID == name)
	vmName := req.VmId

	// Stop domain if running
	state, err := s.libvirtMgr.GetDomainState(vmName)
	if err == nil && state != libvirt.DOMAIN_SHUTOFF {
		if err := s.libvirtMgr.StopDomain(vmName, true); err != nil {
			log.Printf("Warning: failed to stop domain before deletion: %v", err)
		}
	}

	// Delete domain
	if err := s.libvirtMgr.DeleteDomain(vmName); err != nil {
		return nil, status.Errorf(codes.Internal, "failed to delete domain: %v", err)
	}

	log.Printf("Deleted VM %s", vmName)

	return &node.DeleteVmResponse{}, nil
}

func (s *Server) StartVm(ctx context.Context, req *node.StartVmRequest) (*node.StartVmResponse, error) {
	if req.VmId == "" {
		return nil, status.Error(codes.InvalidArgument, "vm_id is required")
	}

	vmName := req.VmId
	if err := s.libvirtMgr.StartDomain(vmName); err != nil {
		return nil, status.Errorf(codes.Internal, "failed to start domain: %v", err)
	}

	log.Printf("Started VM %s", vmName)

	state, err := s.libvirtMgr.GetDomainState(vmName)
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to get domain state: %v", err)
	}

	return &node.StartVmResponse{
		Status: &node.VmStatus{
			Id:    req.VmId,
			State: libvirtDomainStateToProto(state),
		},
	}, nil
}

func (s *Server) StopVm(ctx context.Context, req *node.StopVmRequest) (*node.StopVmResponse, error) {
	if req.VmId == "" {
		return nil, status.Error(codes.InvalidArgument, "vm_id is required")
	}

	vmName := req.VmId
	if err := s.libvirtMgr.StopDomain(vmName, req.Force); err != nil {
		return nil, status.Errorf(codes.Internal, "failed to stop domain: %v", err)
	}

	log.Printf("Stopped VM %s", vmName)

	state, err := s.libvirtMgr.GetDomainState(vmName)
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to get domain state: %v", err)
	}

	return &node.StopVmResponse{
		Status: &node.VmStatus{
			Id:    req.VmId,
			State: libvirtDomainStateToProto(state),
		},
	}, nil
}

func (s *Server) RebootVm(ctx context.Context, req *node.RebootVmRequest) (*node.RebootVmResponse, error) {
	if req.VmId == "" {
		return nil, status.Error(codes.InvalidArgument, "vm_id is required")
	}

	vmName := req.VmId
	if err := s.libvirtMgr.RebootDomain(vmName, req.Force); err != nil {
		return nil, status.Errorf(codes.Internal, "failed to reboot domain: %v", err)
	}

	log.Printf("Rebooted VM %s", vmName)

	state, err := s.libvirtMgr.GetDomainState(vmName)
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to get domain state: %v", err)
	}

	return &node.RebootVmResponse{
		Status: &node.VmStatus{
			Id:    req.VmId,
			State: libvirtDomainStateToProto(state),
		},
	}, nil
}

func (s *Server) GetVm(ctx context.Context, req *node.GetVmRequest) (*node.GetVmResponse, error) {
	if req.VmId == "" {
		return nil, status.Error(codes.InvalidArgument, "vm_id is required")
	}

	// Get full domain info from libvirt
	vmSpec, state, err := s.libvirtMgr.GetDomainInfo(req.VmId)
	if err != nil {
		return nil, status.Errorf(codes.NotFound, "vm not found: %v", err)
	}

	// Convert disks
	disks := make([]*node.Disk, 0, len(vmSpec.Disks))
	for _, disk := range vmSpec.Disks {
		disks = append(disks, &node.Disk{
			Name:          disk.Name,
			BackendHandle: disk.BackendHandle,
			Bus:           disk.Bus,
			Device:        disk.Device,
		})
	}

	// Convert NICs
	nics := make([]*node.Nic, 0, len(vmSpec.NICs))
	for _, nic := range vmSpec.NICs {
		nics = append(nics, &node.Nic{
			Network:    nic.Network,
			Model:      nic.Model,
			MacAddress: nic.MACAddress,
		})
	}

	// Build response with full spec
	return &node.GetVmResponse{
		Spec: &node.VmSpec{
			Id:          vmSpec.ID,
			Name:        vmSpec.Name,
			Cpu:         vmSpec.CPU,
			MemoryBytes: vmSpec.MemoryBytes,
			Disks:       disks,
			Nics:        nics,
		},
		Status: &node.VmStatus{
			Id:    vmSpec.ID,
			State: libvirtDomainStateToProto(state),
		},
	}, nil
}

func (s *Server) ListVms(ctx context.Context, req *node.ListVmsRequest) (*node.ListVmsResponse, error) {
	domains, err := s.libvirtMgr.ListAllDomains()
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to list domains: %v", err)
	}

	vms := make([]*node.VmInfo, 0, len(domains))
	for _, domain := range domains {
		name, err := domain.GetName()
		if err != nil {
			log.Printf("Warning: failed to get domain name: %v", err)
			continue
		}

		uuid, err := domain.GetUUIDString()
		if err != nil {
			log.Printf("Warning: failed to get domain UUID: %v", err)
			continue
		}

		state, _, err := domain.GetState()
		if err != nil {
			log.Printf("Warning: failed to get domain state: %v", err)
			continue
		}

		// Get VM info (CPU, memory)
		info, err := domain.GetInfo()
		if err != nil {
			log.Printf("Warning: failed to get domain info: %v", err)
			continue
		}

		vmInfo := &node.VmInfo{
			Id:          uuid,
			Name:        name,
			State:       libvirtDomainStateToProto(state),
			Cpu:         int32(info.NrVirtCpu),
			MemoryBytes: int64(info.MaxMem * 1024), // Convert from KiB to bytes
		}

		vms = append(vms, vmInfo)
		domain.Free()
	}

	return &node.ListVmsResponse{
		Vms: vms,
	}, nil
}

// NodeStorage implementation

func (s *Server) CreateVolume(ctx context.Context, req *node.CreateVolumeRequest) (*node.CreateVolumeResponse, error) {
	driver, err := s.storageReg.Get(req.StorageClass)
	if err != nil {
		return nil, status.Errorf(codes.NotFound, "storage driver not found: %v", err)
	}

	spec := storage.VolumeSpecOnNode{
		VolumeID:     req.VolumeId,
		StorageClass: req.StorageClass,
		SizeBytes:    req.SizeBytes,
		Parameters:   req.Parameters,
	}

	backendHandle, err := driver.Create(ctx, spec)
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to create volume: %v", err)
	}

	log.Printf("Created volume %s with backend handle %s", req.VolumeId, backendHandle)

	return &node.CreateVolumeResponse{
		BackendHandle: backendHandle,
	}, nil
}

func (s *Server) DeleteVolume(ctx context.Context, req *node.DeleteVolumeRequest) (*node.DeleteVolumeResponse, error) {
	if req.BackendHandle == "" {
		return nil, status.Error(codes.InvalidArgument, "backend_handle is required")
	}

	// Determine driver from backend handle
	// This is a simplified approach - in production, track volume -> driver mapping
	var driver storage.VolumeDriver
	if _, err := os.Stat(req.BackendHandle); err == nil {
		// File-based volume
		driver, err = s.storageReg.Get("local-dir")
		if err != nil {
			return nil, status.Errorf(codes.Internal, "local-dir driver not found: %v", err)
		}
	} else {
		// Try LVM
		driver, err = s.storageReg.Get("local-lvm")
		if err != nil {
			return nil, status.Errorf(codes.Internal, "failed to determine storage driver: %v", err)
		}
	}

	if err := driver.Delete(ctx, req.BackendHandle); err != nil {
		return nil, status.Errorf(codes.Internal, "failed to delete volume: %v", err)
	}

	log.Printf("Deleted volume with backend handle %s", req.BackendHandle)

	return &node.DeleteVolumeResponse{}, nil
}

func (s *Server) AttachVolume(ctx context.Context, req *node.AttachVolumeRequest) (*node.AttachVolumeResponse, error) {
	// Volume attachment is handled by libvirt XML, so this is mostly a no-op
	// The volume should already be attached when the VM domain is created/updated
	log.Printf("Volume %s attached to VM %s as %s", req.BackendHandle, req.VmId, req.TargetDevice)
	return &node.AttachVolumeResponse{}, nil
}

func (s *Server) DetachVolume(ctx context.Context, req *node.DetachVolumeRequest) (*node.DetachVolumeResponse, error) {
	// Volume detachment is handled by libvirt XML
	log.Printf("Volume %s detached from VM %s", req.BackendHandle, req.VmId)
	return &node.DetachVolumeResponse{}, nil
}

// NodeInfo implementation

func (s *Server) GetNodeInfo(ctx context.Context, req *node.GetNodeInfoRequest) (*node.GetNodeInfoResponse, error) {
	// Get CPU info
	cpuCores, err := getCPUCoreCount()
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to get CPU info: %v", err)
	}

	// Get memory info
	memoryBytes, err := getTotalMemory()
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to get memory info: %v", err)
	}

	// Get storage backends
	storageBackends := make([]*node.StorageBackend, 0)
	for _, driverName := range s.storageReg.List() {
		driver, err := s.storageReg.Get(driverName)
		if err != nil {
			continue
		}

		total, free, err := driver.Capacity(ctx)
		if err != nil {
			log.Printf("Warning: failed to get capacity for driver %s: %v", driverName, err)
			continue
		}

		// Determine if shared (simplified - check driver name)
		shared := false
		if driverName == "linstor" || driverName == "san-iscsi" || driverName == "san-fc" {
			shared = true
		}

		storageBackends = append(storageBackends, &node.StorageBackend{
			StorageClass: driverName,
			TotalBytes:   int64(total),
			FreeBytes:    int64(free),
			Shared:       shared,
		})
	}

	// TODO: Get actual usage (running VMs)
	usage := &node.NodeUsage{
		CpuCoresUsed:    0, // TODO: calculate from running VMs
		MemoryBytesUsed: 0, // TODO: calculate from running VMs
	}

	return &node.GetNodeInfoResponse{
		NodeId:   s.nodeID,
		Hostname: s.hostname,
		Capacity: &node.NodeCapacity{
			CpuCores:    int32(cpuCores),
			MemoryBytes: memoryBytes,
		},
		Usage:           usage,
		StorageBackends: storageBackends,
	}, nil
}

// Helper functions

func libvirtDomainStateToProto(state libvirt.DomainState) node.VmState {
	switch state {
	case libvirt.DOMAIN_RUNNING, libvirt.DOMAIN_BLOCKED:
		return node.VmState_VM_STATE_RUNNING
	case libvirt.DOMAIN_PAUSED:
		return node.VmState_VM_STATE_PAUSED
	case libvirt.DOMAIN_SHUTDOWN, libvirt.DOMAIN_SHUTOFF, libvirt.DOMAIN_CRASHED:
		return node.VmState_VM_STATE_STOPPED
	default:
		return node.VmState_VM_STATE_UNKNOWN
	}
}

func getCPUCoreCount() (int, error) {
	cmd := exec.Command("nproc")
	output, err := cmd.Output()
	if err != nil {
		return 0, err
	}

	var cores int
	if _, err := fmt.Sscanf(string(output), "%d", &cores); err != nil {
		return 0, err
	}

	return cores, nil
}

func getTotalMemory() (int64, error) {
	cmd := exec.Command("free", "-b")
	output, err := cmd.Output()
	if err != nil {
		return 0, err
	}

	// Parse "Mem:  total        used        free..."
	lines := strings.Split(string(output), "\n")
	if len(lines) < 2 {
		return 0, fmt.Errorf("unexpected free output")
	}

	var total int64
	if _, err := fmt.Sscanf(lines[1], "Mem: %d", &total); err != nil {
		return 0, err
	}

	return total, nil
}
