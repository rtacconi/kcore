package libvirt

import (
	"fmt"
	"strings"

	libvirt "github.com/libvirt/libvirt-go"
	libvirtxml "github.com/libvirt/libvirt-go-xml"
)

type Manager struct {
	conn *libvirt.Connect
}

func New() (*Manager, error) {
	conn, err := libvirt.NewConnect("qemu:///system")
	if err != nil {
		return nil, fmt.Errorf("failed to connect to libvirt: %w", err)
	}

	return &Manager{conn: conn}, nil
}

func (m *Manager) Close() error {
	if m.conn != nil {
		_, err := m.conn.Close()
		return err
	}
	return nil
}

// DomainToVMState converts libvirt domain state to VM state
func DomainToVMState(state libvirt.DomainState) string {
	switch state {
	case libvirt.DOMAIN_RUNNING, libvirt.DOMAIN_BLOCKED:
		return "running"
	case libvirt.DOMAIN_PAUSED:
		return "paused"
	case libvirt.DOMAIN_SHUTDOWN, libvirt.DOMAIN_SHUTOFF, libvirt.DOMAIN_CRASHED:
		return "stopped"
	default:
		return "unknown"
	}
}

// BuildDomainXML builds libvirt domain XML from VM spec
func BuildDomainXML(spec *VMSpec) (*libvirtxml.Domain, error) {
	domain := &libvirtxml.Domain{
		Type: "kvm",
		Name: spec.Name,
		UUID: spec.ID,
		Memory: &libvirtxml.DomainMemory{
			Value: uint(spec.MemoryBytes / 1024), // Convert to KiB
			Unit:  "KiB",
		},
		VCPU: &libvirtxml.DomainVCPU{
			Value: uint(spec.CPU),
		},
		OS: &libvirtxml.DomainOS{
			Type: &libvirtxml.DomainOSType{
				Arch: "x86_64",
				Type: "hvm",
			},
		},
		Features: &libvirtxml.DomainFeatureList{
			ACPI: &libvirtxml.DomainFeature{},
			APIC: &libvirtxml.DomainFeatureAPIC{},
		},
		CPU: &libvirtxml.DomainCPU{
			Mode: "host-passthrough",
		},
		Clock: &libvirtxml.DomainClock{
			Offset: "utc",
			Timer: []libvirtxml.DomainTimer{
				{Name: "rtc", Track: "wall"},
				{Name: "pit", TickPolicy: "delay"},
				{Name: "hpet", Present: "yes"},
			},
		},
		Devices: &libvirtxml.DomainDeviceList{},
	}

	// Add disks
	domain.Devices.Disks = make([]libvirtxml.DomainDisk, 0, len(spec.Disks))
	for _, disk := range spec.Disks {
		isCloudInit := disk.Name == "cloud-init" || strings.HasSuffix(strings.ToLower(disk.BackendHandle), ".iso")

		devType := "disk"
		driverType := "qcow2"
		bus := disk.Bus
		if isCloudInit {
			devType = "cdrom"
			driverType = "raw"
			bus = "sata"
			disk.Device = "sda"
		}

		libvirtDisk := libvirtxml.DomainDisk{
			Device: devType,
			Driver: &libvirtxml.DomainDiskDriver{
				Name: "qemu",
				Type: driverType,
			},
			Target: &libvirtxml.DomainDiskTarget{
				Dev: disk.Device,
				Bus: bus,
			},
		}

		// Determine if it's a file or block device
		if strings.HasPrefix(disk.BackendHandle, "/dev/") {
			libvirtDisk.Source = &libvirtxml.DomainDiskSource{
				Block: &libvirtxml.DomainDiskSourceBlock{
					Dev: disk.BackendHandle,
				},
			}
		} else {
			libvirtDisk.Source = &libvirtxml.DomainDiskSource{
				File: &libvirtxml.DomainDiskSourceFile{
					File: disk.BackendHandle,
				},
			}
		}

		if isCloudInit {
			libvirtDisk.ReadOnly = &libvirtxml.DomainDiskReadOnly{}
		}

		domain.Devices.Disks = append(domain.Devices.Disks, libvirtDisk)
	}

	// Add network interfaces
	domain.Devices.Interfaces = make([]libvirtxml.DomainInterface, 0, len(spec.NICs))
	for _, nic := range spec.NICs {
		iface := libvirtxml.DomainInterface{
			Model: &libvirtxml.DomainInterfaceModel{
				Type: nic.Model,
			},
		}

		// Use libvirt Network mode (supports both libvirt networks and bridges).
		// If network name starts with "br" or contains "/", assume it's a bridge
		// (e.g. br0, br1). Otherwise, treat it as a libvirt network name.
		if strings.HasPrefix(nic.Network, "br") || strings.Contains(nic.Network, "/") {
			// Bridge mode – Linux bridges (br0, br1, etc.)
			iface.Source = &libvirtxml.DomainInterfaceSource{
				Bridge: &libvirtxml.DomainInterfaceSourceBridge{
					Bridge: nic.Network,
				},
			}
		} else {
			// Network mode (for libvirt networks like "default", "private", etc.)
			iface.Source = &libvirtxml.DomainInterfaceSource{
				Network: &libvirtxml.DomainInterfaceSourceNetwork{
					Network: nic.Network,
				},
			}
		}

		if nic.MACAddress != "" {
			iface.MAC = &libvirtxml.DomainInterfaceMAC{
				Address: nic.MACAddress,
			}
		}

		domain.Devices.Interfaces = append(domain.Devices.Interfaces, iface)
	}

	// Add console and serial
	portZero := uint(0)
	domain.Devices.Consoles = []libvirtxml.DomainConsole{
		{
			// Use a PTY-backed console so `virsh console` works as expected.
			Source: &libvirtxml.DomainChardevSource{
				Pty: &libvirtxml.DomainChardevSourcePty{},
			},
			Target: &libvirtxml.DomainConsoleTarget{
				Type: "serial",
				Port: &portZero,
			},
		},
	}

	domain.Devices.Serials = []libvirtxml.DomainSerial{
		{
			// Match the console with a PTY-backed serial device.
			Source: &libvirtxml.DomainChardevSource{
				Pty: &libvirtxml.DomainChardevSourcePty{},
			},
			Target: &libvirtxml.DomainSerialTarget{
				Port: &portZero,
			},
		},
	}

	// Graphics (VNC/Spice) - commented out for now, can be added later if needed
	// domain.Devices.Graphics = []libvirtxml.DomainGraphics{
	// 	{
	// 		VNC: &libvirtxml.DomainGraphicsVNC{
	// 			Port:     -1, // Auto-assign
	// 			AutoPort: "yes",
	// 			Listen:   "0.0.0.0",
	// 		},
	// 	},
	// }

	return domain, nil
}

// VMSpec represents a VM specification for libvirt
type VMSpec struct {
	ID          string
	Name        string
	CPU         int32
	MemoryBytes int64
	Disks       []DiskSpec
	NICs        []NICSpec
}

type DiskSpec struct {
	Name          string
	BackendHandle string
	Bus           string
	Device        string
}

type NICSpec struct {
	Network     string
	Model       string
	MACAddress  string
	InterfaceID string // Optional iface-id for bridge port identification
}

// CreateDomain creates a libvirt domain from XML
func (m *Manager) CreateDomain(xml string) (*libvirt.Domain, error) {
	// Define the domain (makes it persistent)
	domain, err := m.conn.DomainDefineXML(xml)
	if err != nil {
		return nil, fmt.Errorf("failed to define domain: %w", err)
	}

	// Optionally start the domain
	if err := domain.Create(); err != nil {
		// If start fails, clean up the definition
		domain.Undefine()
		return nil, fmt.Errorf("failed to start domain: %w", err)
	}

	return domain, nil
}

// GetDomain retrieves a domain by name or UUID
func (m *Manager) GetDomain(nameOrUUID string) (*libvirt.Domain, error) {
	// Try by name first
	domain, err := m.conn.LookupDomainByName(nameOrUUID)
	if err == nil {
		return domain, nil
	}

	// Try by UUID
	domain, err = m.conn.LookupDomainByUUIDString(nameOrUUID)
	if err == nil {
		return domain, nil
	}

	// Try to find by searching all domains for matching name
	domains, err := m.ListAllDomains()
	if err != nil {
		return nil, fmt.Errorf("domain not found: %s", nameOrUUID)
	}

	for _, dom := range domains {
		// Check if name matches
		name, err := dom.GetName()
		if err != nil {
			continue
		}
		if name == nameOrUUID {
			return &dom, nil
		}

		// Check if UUID matches
		uuid, err := dom.GetUUIDString()
		if err != nil {
			continue
		}
		if uuid == nameOrUUID {
			return &dom, nil
		}
	}

	return nil, fmt.Errorf("domain not found: %s", nameOrUUID)
}

// ListAllDomains lists all domains (running and stopped)
func (m *Manager) ListAllDomains() ([]libvirt.Domain, error) {
	domains, err := m.conn.ListAllDomains(libvirt.CONNECT_LIST_DOMAINS_ACTIVE | libvirt.CONNECT_LIST_DOMAINS_INACTIVE)
	if err != nil {
		return nil, fmt.Errorf("failed to list domains: %w", err)
	}
	return domains, nil
}

// StartDomain starts a domain
func (m *Manager) StartDomain(name string) error {
	domain, err := m.GetDomain(name)
	if err != nil {
		return err
	}
	defer domain.Free()

	return domain.Create()
}

// StopDomain stops a domain
func (m *Manager) StopDomain(name string, force bool) error {
	domain, err := m.GetDomain(name)
	if err != nil {
		return err
	}
	defer domain.Free()

	if force {
		return domain.Destroy()
	}
	return domain.Shutdown()
}

// RebootDomain reboots a domain
func (m *Manager) RebootDomain(name string, force bool) error {
	domain, err := m.GetDomain(name)
	if err != nil {
		return err
	}
	defer domain.Free()

	if force {
		return domain.Reset(0)
	}
	return domain.Reboot(libvirt.DOMAIN_REBOOT_DEFAULT)
}

// DeleteDomain deletes a domain
func (m *Manager) DeleteDomain(name string) error {
	domain, err := m.GetDomain(name)
	if err != nil {
		return err
	}
	defer domain.Free()

	// Undefine the domain
	return domain.UndefineFlags(libvirt.DOMAIN_UNDEFINE_NVRAM)
}

// GetDomainState gets the state of a domain
func (m *Manager) GetDomainState(name string) (libvirt.DomainState, error) {
	domain, err := m.GetDomain(name)
	if err != nil {
		return 0, err
	}
	defer domain.Free()

	state, _, err := domain.GetState()
	return state, err
}

// GetDomainInfo retrieves full domain information
func (m *Manager) GetDomainInfo(nameOrUUID string) (*VMSpec, libvirt.DomainState, error) {
	domain, err := m.GetDomain(nameOrUUID)
	if err != nil {
		return nil, 0, err
	}
	defer domain.Free()

	// Get basic info
	info, err := domain.GetInfo()
	if err != nil {
		return nil, 0, fmt.Errorf("failed to get domain info: %w", err)
	}

	// Get UUID and name
	uuid, err := domain.GetUUIDString()
	if err != nil {
		return nil, 0, fmt.Errorf("failed to get UUID: %w", err)
	}

	name, err := domain.GetName()
	if err != nil {
		return nil, 0, fmt.Errorf("failed to get name: %w", err)
	}

	// Get state
	state, _, err := domain.GetState()
	if err != nil {
		return nil, 0, fmt.Errorf("failed to get state: %w", err)
	}

	// TODO: Parse XML to get disks and NICs
	// For now, return basic info
	spec := &VMSpec{
		ID:          uuid,
		Name:        name,
		CPU:         int32(info.NrVirtCpu),
		MemoryBytes: int64(info.MaxMem * 1024), // Convert from KiB to bytes
		Disks:       []DiskSpec{},
		NICs:        []NICSpec{},
	}

	return spec, state, nil
}

// UpdateDomain updates a domain's XML
func (m *Manager) UpdateDomain(xml string) error {
	// Parse XML to get domain name
	domainXML := &libvirtxml.Domain{}
	if err := domainXML.Unmarshal(xml); err != nil {
		return fmt.Errorf("failed to parse domain XML: %w", err)
	}

	// Get existing domain
	domain, err := m.GetDomain(domainXML.Name)
	if err != nil {
		return err
	}
	defer domain.Free()

	// Undefine and redefine
	if err := domain.UndefineFlags(libvirt.DOMAIN_UNDEFINE_NVRAM); err != nil {
		return fmt.Errorf("failed to undefine domain: %w", err)
	}

	_, err = m.conn.DomainDefineXML(xml)
	return err
}
