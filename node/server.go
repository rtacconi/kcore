package node

import (
	"context"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
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
	node.UnimplementedNodeAdminServer

	libvirtMgr *libvirtmgr.Manager
	storageReg *storage.DriverRegistry
	networks   map[string]string
	nodeID     string
	hostname   string

	Automator *AutomatorServer
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

// NodeAdmin implementation

// ApplyNixConfig writes the provided configuration.nix to /etc/nixos/configuration.nix
// and optionally runs `nixos-rebuild switch`. This is intentionally very simple and
// unauthenticated for now, for development and lab use only.
func (s *Server) ApplyNixConfig(ctx context.Context, req *node.ApplyNixConfigRequest) (*node.ApplyNixConfigResponse, error) {
	if req.GetConfigurationNix() == "" {
		return &node.ApplyNixConfigResponse{
			Success: false,
			Message: "configuration_nix is empty",
		}, nil
	}

	const cfgPath = "/etc/nixos/configuration.nix"

	// Ensure directory exists
	if err := os.MkdirAll(filepath.Dir(cfgPath), 0755); err != nil {
		return &node.ApplyNixConfigResponse{
			Success: false,
			Message: fmt.Sprintf("failed to create /etc/nixos: %v", err),
		}, nil
	}

	// Write configuration file
	if err := os.WriteFile(cfgPath, []byte(req.GetConfigurationNix()), 0644); err != nil {
		return &node.ApplyNixConfigResponse{
			Success: false,
			Message: fmt.Sprintf("failed to write %s: %v", cfgPath, err),
		}, nil
	}

	// Optionally trigger a rebuild
	if req.GetRebuild() {
		cmd := exec.CommandContext(ctx, "nixos-rebuild", "switch")
		output, err := cmd.CombinedOutput()
		if err != nil {
			return &node.ApplyNixConfigResponse{
				Success: false,
				Message: fmt.Sprintf("nixos-rebuild switch failed: %v (output: %s)", err, strings.TrimSpace(string(output))),
			}, nil
		}
	}

	return &node.ApplyNixConfigResponse{
		Success: true,
		Message: "configuration applied",
	}, nil
}

// Image handling helpers

const (
	imagesCacheDir = "/var/lib/kcore/images"
	disksDir       = "/var/lib/kcore/disks"
)

// downloadImage downloads an image from a URI to the cache directory
func (s *Server) downloadImage(ctx context.Context, uri string) (string, error) {
	// Ensure cache directory exists
	if err := os.MkdirAll(imagesCacheDir, 0755); err != nil {
		return "", fmt.Errorf("failed to create images cache directory: %w", err)
	}

	// Generate filename from URI
	filename := filepath.Base(uri)
	cachePath := filepath.Join(imagesCacheDir, filename)

	// Check if already downloaded
	if _, err := os.Stat(cachePath); err == nil {
		log.Printf("Using cached image: %s", cachePath)
		return cachePath, nil
	}

	log.Printf("Downloading image from %s...", uri)

	// Download the image
	resp, err := http.Get(uri)
	if err != nil {
		return "", fmt.Errorf("failed to download image: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("failed to download image: HTTP %d", resp.StatusCode)
	}

	// Create temporary file
	tmpPath := cachePath + ".tmp"
	out, err := os.Create(tmpPath)
	if err != nil {
		return "", fmt.Errorf("failed to create file: %w", err)
	}
	defer out.Close()

	// Copy data
	_, err = io.Copy(out, resp.Body)
	if err != nil {
		os.Remove(tmpPath)
		return "", fmt.Errorf("failed to write image: %w", err)
	}

	// Rename to final name
	if err := os.Rename(tmpPath, cachePath); err != nil {
		os.Remove(tmpPath)
		return "", fmt.Errorf("failed to rename image: %w", err)
	}

	log.Printf("Image downloaded successfully: %s", cachePath)
	return cachePath, nil
}

// prepareVMImage creates a COW (copy-on-write) disk from a base image for a VM
func (s *Server) prepareVMImage(vmID, baseImagePath string) (string, error) {
	// Ensure disks directory exists
	if err := os.MkdirAll(disksDir, 0755); err != nil {
		return "", fmt.Errorf("failed to create disks directory: %w", err)
	}

	// Create COW disk path
	vmDiskPath := filepath.Join(disksDir, fmt.Sprintf("%s-disk.qcow2", vmID))

	// Create COW disk using qemu-img
	// This creates a thin-provisioned disk backed by the base image.
	// On NixOS, qemu-img lives at /run/current-system/sw/bin/qemu-img, which
	// may not be on the restricted systemd service PATH, so we call it by
	// absolute path.
	cmd := exec.Command("/run/current-system/sw/bin/qemu-img", "create", "-f", "qcow2", "-F", "qcow2", "-b", baseImagePath, vmDiskPath)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("failed to create COW disk: %w (output: %s)", err, string(output))
	}

	log.Printf("Created VM disk: %s (backed by %s)", vmDiskPath, baseImagePath)
	return vmDiskPath, nil
}

// detectImageFlavor returns a lightweight distro hint derived from image path/URI.
func detectImageFlavor(imageRef string) string {
	ref := strings.ToLower(imageRef)
	switch {
	case strings.Contains(ref, "ubuntu"):
		return "ubuntu"
	case strings.Contains(ref, "debian"):
		return "debian"
	default:
		return "generic"
	}
}

func buildCloudInitUserData(imageRef string, enableKcoreLogin bool) (string, string) {
	flavor := detectImageFlavor(imageRef)
	userData := `#cloud-config
ssh_pwauth: false
disable_root: true
runcmd:
  - [ systemctl, enable, --now, serial-getty@ttyS0.service ]
`

	if enableKcoreLogin {
		loginEntries := []string{
			"root:kcore",
			"kcore:kcore",
		}
		switch flavor {
		case "debian":
			loginEntries = append(loginEntries, "debian:kcore")
		case "ubuntu":
			loginEntries = append(loginEntries, "ubuntu:kcore")
		}

		// Cloud-config for enabled login mode:
		// - stable admin account (kcore/kcore)
		// - distro-default user password for convenience
		// - root password + serial console
		userData = fmt.Sprintf(`#cloud-config
users:
  - default
  - name: kcore
    groups: [sudo]
    shell: /bin/bash
    lock_passwd: false
    plain_text_passwd: kcore
    sudo: "ALL=(ALL) NOPASSWD:ALL"
chpasswd:
  list: |
    %s
  plaintext: true
  expire: False
ssh_pwauth: True
disable_root: false
runcmd:
  - [ systemctl, enable, --now, serial-getty@ttyS0.service ]
`, strings.Join(loginEntries, "\n    "))
	}

	return userData, flavor
}

// prepareCloudInitDisk creates a NoCloud seed ISO for cloud-init so that the
// guest can be configured with known credentials and basic metadata.
// If customUserData is non-empty, it is used verbatim instead of the generated default.
func (s *Server) prepareCloudInitDisk(vmID, vmName, imageRef string, enableKcoreLogin bool, customUserData string) (string, error) {
	baseDir := filepath.Join("/var/lib/kcore/cloud-init", vmID)
	if err := os.MkdirAll(baseDir, 0700); err != nil {
		return "", fmt.Errorf("failed to create cloud-init dir: %w", err)
	}

	userDataPath := filepath.Join(baseDir, "user-data")
	metaDataPath := filepath.Join(baseDir, "meta-data")
	seedPath := filepath.Join(baseDir, "seed.iso")

	var userData, flavor string
	if customUserData != "" {
		userData = customUserData
		flavor = "custom"
	} else {
		userData, flavor = buildCloudInitUserData(imageRef, enableKcoreLogin)
	}

	metaData := fmt.Sprintf("instance-id: %s\nlocal-hostname: %s\n", vmID, vmName)

	if err := os.WriteFile(userDataPath, []byte(userData), 0600); err != nil {
		return "", fmt.Errorf("failed to write user-data: %w", err)
	}
	if err := os.WriteFile(metaDataPath, []byte(metaData), 0600); err != nil {
		return "", fmt.Errorf("failed to write meta-data: %w", err)
	}

	// Use cloud-localds from cloud-utils to build the seed ISO.
	// Prefer the per-user Nix profile location (how we installed it on the node),
	// and fall back to the system profile if present.
	cloudLocalDS := "/root/.nix-profile/bin/cloud-localds"
	if _, err := os.Stat(cloudLocalDS); err != nil {
		cloudLocalDS = "/run/current-system/sw/bin/cloud-localds"
	}
	cmd := exec.Command(cloudLocalDS, "-v", seedPath, userDataPath, metaDataPath)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("failed to create cloud-init seed ISO: %w (output: %s)", err, strings.TrimSpace(string(output)))
	}

	log.Printf("Created cloud-init seed ISO for VM %s at %s (image flavor: %s, enable-kcore-login=%t)", vmName, seedPath, flavor, enableKcoreLogin)
	return seedPath, nil
}

// NodeCompute implementation

func (s *Server) CreateVm(ctx context.Context, req *node.CreateVmRequest) (*node.CreateVmResponse, error) {
	spec := req.Spec
	if spec == nil {
		return nil, status.Error(codes.InvalidArgument, "vm spec is required")
	}

	// Handle boot image if provided
	var bootDiskPath string
	var imageRef string
	if req.ImageUri != "" || req.ImagePath != "" {
		var baseImagePath string
		var err error

		if req.ImageUri != "" {
			// Download image from URI
			baseImagePath, err = s.downloadImage(ctx, req.ImageUri)
			if err != nil {
				return nil, status.Errorf(codes.Internal, "failed to download image: %v", err)
			}
		} else {
			// Use local image path
			baseImagePath = req.ImagePath
			if _, err := os.Stat(baseImagePath); err != nil {
				return nil, status.Errorf(codes.InvalidArgument, "image file not found: %s", baseImagePath)
			}
		}

		// Create COW disk for this VM
		bootDiskPath, err = s.prepareVMImage(spec.Id, baseImagePath)
		if err != nil {
			return nil, status.Errorf(codes.Internal, "failed to prepare VM image: %v", err)
		}
		imageRef = baseImagePath
		log.Printf("VM %s will boot from %s", spec.Name, bootDiskPath)
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

	// Add boot disk first if we have an image
	if bootDiskPath != "" {
		libvirtSpec.Disks = append(libvirtSpec.Disks, libvirtmgr.DiskSpec{
			Name:          "vda",
			BackendHandle: bootDiskPath,
			Bus:           "virtio",
			Device:        "vda",
		})

		// When we boot from an image, also attach a cloud-init NoCloud seed ISO
		// so we can guarantee a known root password and basic metadata, without
		// needing external cloud-init infrastructure.
		if seedPath, err := s.prepareCloudInitDisk(spec.Id, spec.Name, imageRef, spec.EnableKcoreLogin, req.CloudInitUserData); err != nil {
			log.Printf("Warning: failed to prepare cloud-init disk for VM %s: %v", spec.Name, err)
		} else {
			// Attach the seed ISO as a separate raw data disk. Cloud-init NoCloud
			// detects the `cidata` filesystem label from regular block devices.
			libvirtSpec.Disks = append(libvirtSpec.Disks, libvirtmgr.DiskSpec{
				Name:          "cloud-init",
				BackendHandle: seedPath,
				Bus:           "virtio",
				Device:        "vdb",
			})
		}
	}
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

		model := nic.Model
		if model == "" {
			model = "virtio" // Default to virtio
		}

		libvirtSpec.NICs = append(libvirtSpec.NICs, libvirtmgr.NICSpec{
			Network:    bridgeName,
			Model:      model,
			MACAddress: nic.MacAddress,
		})
	}

	// If no NICs specified, add default NIC on libvirt "default" network with DHCP
	if len(libvirtSpec.NICs) == 0 {
		// Use network mapping if configured, otherwise use libvirt's "default" network
		defaultNetwork := s.networks["default"]
		if defaultNetwork == "" {
			defaultNetwork = "default" // Use libvirt's default network (NAT with DHCP)
		}

		libvirtSpec.NICs = append(libvirtSpec.NICs, libvirtmgr.NICSpec{
			Network:    defaultNetwork,
			Model:      "virtio",
			MACAddress: "", // Let libvirt generate MAC
		})
		log.Printf("Added default NIC on network '%s' (DHCP provided by libvirt)", defaultNetwork)
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
			State: node.VmState_VM_STATE_RUNNING,
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
	seedPath := filepath.Join("/var/lib/kcore/cloud-init", spec.Id, "seed.iso")
	seedExists := false
	if _, err := os.Stat(seedPath); err == nil {
		seedExists = true
		if _, err := s.prepareCloudInitDisk(spec.Id, spec.Name, "", spec.EnableKcoreLogin, ""); err != nil {
			log.Printf("Warning: failed to refresh cloud-init seed for VM %s: %v", spec.Name, err)
		}
	}
	hasCloudInitDisk := false
	for _, disk := range spec.Disks {
		if disk.Name == "cloud-init" {
			hasCloudInitDisk = true
		}
		libvirtSpec.Disks = append(libvirtSpec.Disks, libvirtmgr.DiskSpec{
			Name:          disk.Name,
			BackendHandle: disk.BackendHandle,
			Bus:           disk.Bus,
			Device:        disk.Device,
		})
	}
	if seedExists && !hasCloudInitDisk {
		libvirtSpec.Disks = append(libvirtSpec.Disks, libvirtmgr.DiskSpec{
			Name:          "cloud-init",
			BackendHandle: seedPath,
			Bus:           "virtio",
			Device:        "vdb",
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

// Image management

func (s *Server) PullImage(ctx context.Context, req *node.PullImageRequest) (*node.PullImageResponse, error) {
	if req.Uri == "" {
		return nil, status.Error(codes.InvalidArgument, "uri is required")
	}

	// Download the image (uses cache if already exists)
	imagePath, err := s.downloadImage(ctx, req.Uri)
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to download image: %v", err)
	}

	// Get file info
	fileInfo, err := os.Stat(imagePath)
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to stat image: %v", err)
	}

	// Check if it was cached (by seeing if it existed before we called downloadImage)
	// For simplicity, we'll return false here since downloadImage logs when it uses cache
	cached := false

	log.Printf("Image pulled: %s (%d bytes)", imagePath, fileInfo.Size())

	return &node.PullImageResponse{
		Path:      imagePath,
		SizeBytes: fileInfo.Size(),
		Cached:    cached,
	}, nil
}

func (s *Server) ListImages(ctx context.Context, req *node.ListImagesRequest) (*node.ListImagesResponse, error) {
	// Ensure images directory exists
	if err := os.MkdirAll(imagesCacheDir, 0755); err != nil {
		return nil, status.Errorf(codes.Internal, "failed to access images directory: %v", err)
	}

	// Read directory
	entries, err := os.ReadDir(imagesCacheDir)
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to read images directory: %v", err)
	}

	// Build image list
	var images []*node.ImageInfo
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}

		fullPath := filepath.Join(imagesCacheDir, entry.Name())
		fileInfo, err := os.Stat(fullPath)
		if err != nil {
			log.Printf("Warning: failed to stat %s: %v", fullPath, err)
			continue
		}

		images = append(images, &node.ImageInfo{
			Name:      entry.Name(),
			Path:      fullPath,
			SizeBytes: fileInfo.Size(),
			// CreatedAt can be added if needed
		})
	}

	return &node.ListImagesResponse{
		Images: images,
	}, nil
}

func (s *Server) DeleteImage(ctx context.Context, req *node.DeleteImageRequest) (*node.DeleteImageResponse, error) {
	if req.Name == "" {
		return nil, status.Error(codes.InvalidArgument, "image name is required")
	}

	// Determine full path
	var imagePath string
	if filepath.IsAbs(req.Name) {
		// Full path provided
		imagePath = req.Name
	} else {
		// Just filename, assume it's in images cache directory
		imagePath = filepath.Join(imagesCacheDir, req.Name)
	}

	// Check if image exists
	if _, err := os.Stat(imagePath); os.IsNotExist(err) {
		return &node.DeleteImageResponse{
			Success: false,
			Message: fmt.Sprintf("image not found: %s", req.Name),
		}, nil
	}

	// Check if image is in use (check if any VM disk uses it as backing file)
	if !req.Force {
		inUse, err := s.isImageInUse(imagePath)
		if err != nil {
			return nil, status.Errorf(codes.Internal, "failed to check if image is in use: %v", err)
		}
		if inUse {
			return &node.DeleteImageResponse{
				Success: false,
				Message: fmt.Sprintf("image '%s' is in use by one or more VMs. Use --force to delete anyway", req.Name),
			}, nil
		}
	}

	// Delete the image
	if err := os.Remove(imagePath); err != nil {
		return nil, status.Errorf(codes.Internal, "failed to delete image: %v", err)
	}

	log.Printf("Image deleted: %s", imagePath)

	return &node.DeleteImageResponse{
		Success: true,
		Message: fmt.Sprintf("Image deleted: %s", req.Name),
	}, nil
}

// isImageInUse checks if an image is being used as a backing file by any VM disk
func (s *Server) isImageInUse(imagePath string) (bool, error) {
	// Read all VM disks
	entries, err := os.ReadDir(disksDir)
	if err != nil {
		if os.IsNotExist(err) {
			return false, nil // No disks directory means no VMs using images
		}
		return false, err
	}

	// Check each disk to see if it uses this image as backing file
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}

		diskPath := filepath.Join(disksDir, entry.Name())

		// Use qemu-img to check backing file (see note in prepareVMImage)
		cmd := exec.Command("/run/current-system/sw/bin/qemu-img", "info", diskPath)
		output, err := cmd.CombinedOutput()
		if err != nil {
			// Skip if we can't read this disk
			continue
		}

		// Check if output contains the image path as backing file
		if strings.Contains(string(output), imagePath) {
			return true, nil
		}
	}

	return false, nil
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
