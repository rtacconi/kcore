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
		libvirtDisk := libvirtxml.DomainDisk{
			Device: "disk",
			Driver: &libvirtxml.DomainDiskDriver{
				Name: "qemu",
				Type: "qcow2",
			},
			Target: &libvirtxml.DomainDiskTarget{
				Dev: disk.Device,
				Bus: disk.Bus,
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

		domain.Devices.Disks = append(domain.Devices.Disks, libvirtDisk)
	}

	// Add network interfaces
	domain.Devices.Interfaces = make([]libvirtxml.DomainInterface, 0, len(spec.NICs))
	for _, nic := range spec.NICs {
		iface := libvirtxml.DomainInterface{
			Source: &libvirtxml.DomainInterfaceSource{
				Bridge: &libvirtxml.DomainInterfaceSourceBridge{
					Bridge: nic.Network, // Network name maps to bridge name
				},
			},
			Model: &libvirtxml.DomainInterfaceModel{
				Type: nic.Model,
			},
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
			Target: &libvirtxml.DomainConsoleTarget{
				Type: "serial",
				Port: &portZero,
			},
		},
	}

	domain.Devices.Serials = []libvirtxml.DomainSerial{
		{
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
	Network    string
	Model      string
	MACAddress string
}

// CreateDomain creates a libvirt domain from XML
func (m *Manager) CreateDomain(xml string) (*libvirt.Domain, error) {
	domain, err := m.conn.DomainCreateXML(xml, libvirt.DOMAIN_NONE)
	if err != nil {
		return nil, fmt.Errorf("failed to create domain: %w", err)
	}
	return domain, nil
}

// GetDomain retrieves a domain by name
func (m *Manager) GetDomain(name string) (*libvirt.Domain, error) {
	domain, err := m.conn.LookupDomainByName(name)
	if err != nil {
		return nil, fmt.Errorf("domain not found: %w", err)
	}
	return domain, nil
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
