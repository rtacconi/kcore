package agent

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"time"

	libvirt "libvirt.org/go/libvirt"
	libvirtxml "libvirt.org/go/libvirtxml"

	pb "kcore/gen/api/v1"
)

type Agent struct {
	pb.UnimplementedNodeAgentServer
	conn    *libvirt.Connect
	dataDir string
	node    string
}

func NewAgent(dataDir, node string) (*Agent, error) {
	conn, err := libvirt.NewConnect("qemu:///system")
	if err != nil {
		return nil, fmt.Errorf("connect libvirt: %w", err)
	}
	return &Agent{conn: conn, dataDir: dataDir, node: node}, nil
}

func (a *Agent) Close() { _ = a.conn.Close() }

func (a *Agent) CreateVm(ctx context.Context, req *pb.CreateVmRequest) (*pb.CreateVmResponse, error) {
	id := randomID()
	vmName := req.Spec.Name
	if vmName == "" {
		vmName = "vm-" + id
	}
	vmDir := filepath.Join(a.dataDir, "vms", id)
	if err := os.MkdirAll(vmDir, 0o755); err != nil {
		return nil, fmt.Errorf("mkdir vm dir: %w", err)
	}

	// Disk
	disk0 := filepath.Join(vmDir, "disk0.qcow2")
	sizeBytes := uint64(20 * 1024 * 1024 * 1024)
	if len(req.Spec.Disks) > 0 && req.Spec.Disks[0].SizeBytes > 0 {
		sizeBytes = req.Spec.Disks[0].SizeBytes
	}
	if err := run("qemu-img", "create", "-f", "qcow2", disk0, fmt.Sprintf("%d", sizeBytes)); err != nil {
		return nil, fmt.Errorf("qemu-img create: %w", err)
	}

	// Cloud-init seed
	seedISO := filepath.Join(vmDir, "seed.iso")
	if err := writeCloudInitISO(vmDir, seedISO, req.Spec.CloudInit.GetUserData(), req.Spec.CloudInit.GetMetaData()); err != nil {
		return nil, fmt.Errorf("cloud-init iso: %w", err)
	}

	// Domain XML
	memKiB := req.Spec.Memory.GetBytes() / 1024
	if memKiB == 0 {
		memKiB = 1024 * 1024
	}
	cores := req.Spec.Cpu.GetCores()
	if cores == 0 {
		cores = 2
	}

	domain := &libvirtxml.Domain{
		Type:     "kvm",
		Name:     vmName,
		Memory:   &libvirtxml.DomainMemory{Value: memKiB, Unit: "KiB"},
		VCPU:     &libvirtxml.DomainVCPU{Value: uint(cores)},
		OS:       &libvirtxml.DomainOS{Type: &libvirtxml.DomainOSType{Arch: "x86_64", Machine: "q35", Type: "hvm"}},
		CPU:      &libvirtxml.DomainCPU{Mode: "host-passthrough"},
		Features: &libvirtxml.DomainFeatureList{ACPI: &libvirtxml.DomainFeature{}, APIC: &libvirtxml.DomainFeatureAPIC{}},
		Devices: &libvirtxml.DomainDeviceList{
			Disks: []libvirtxml.DomainDisk{
				{Device: "disk", Driver: &libvirtxml.DomainDiskDriver{Name: "qemu", Type: "qcow2"}, Source: &libvirtxml.DomainDiskSource{File: &libvirtxml.DomainDiskSourceFile{File: disk0}}, Target: &libvirtxml.DomainDiskTarget{Dev: "vda", Bus: "virtio"}},
				{Device: "cdrom", ReadOnly: &libvirtxml.DomainDiskReadOnly{}, Source: &libvirtxml.DomainDiskSource{File: &libvirtxml.DomainDiskSourceFile{File: seedISO}}, Target: &libvirtxml.DomainDiskTarget{Dev: "sda", Bus: "sata"}},
			},
			Interfaces: []libvirtxml.DomainInterface{{Model: &libvirtxml.DomainInterfaceModel{Type: "virtio"}, Source: &libvirtxml.DomainInterfaceSource{Bridge: &libvirtxml.DomainInterfaceSourceBridge{Bridge: "br0"}}}},
			Graphics:   []libvirtxml.DomainGraphic{},
			Consoles:   []libvirtxml.DomainConsole{{Target: &libvirtxml.DomainConsoleTarget{Type: "serial"}}},
		},
		OnPoweroff: "destroy",
		OnCrash:    "destroy",
		OnReboot:   "restart",
	}

	xml, err := domain.Marshal()
	if err != nil {
		return nil, fmt.Errorf("marshal domain xml: %w", err)
	}
	d, err := a.conn.DomainDefineXML(xml)
	if err != nil {
		return nil, fmt.Errorf("define domain: %w", err)
	}
	defer d.Free()

	if err := d.SetAutostart(true); err != nil {
		return nil, fmt.Errorf("autostart: %w", err)
	}
	if err := d.Create(); err != nil {
		return nil, fmt.Errorf("start domain: %w", err)
	}

	uuid, _ := d.GetUUIDString()
	return &pb.CreateVmResponse{Vm: &pb.Vm{Id: uuid, Name: vmName, Node: a.node, Status: "running"}}, nil
}

func (a *Agent) StartVm(ctx context.Context, id *pb.VmId) (*pb.Vm, error) {
	d, err := a.lookup(id.Id)
	if err != nil {
		return nil, err
	}
	defer d.Free()
	running, _ := isRunning(d)
	if !running {
		if err := d.Create(); err != nil {
			return nil, fmt.Errorf("start: %w", err)
		}
	}
	name, _ := d.GetName()
	uuid, _ := d.GetUUIDString()
	return &pb.Vm{Id: uuid, Name: name, Node: a.node, Status: "running"}, nil
}

func (a *Agent) StopVm(ctx context.Context, id *pb.VmId) (*pb.Vm, error) {
	d, err := a.lookup(id.Id)
	if err != nil {
		return nil, err
	}
	defer d.Free()
	_ = d.Shutdown()
	deadline := time.Now().Add(20 * time.Second)
	for time.Now().Before(deadline) {
		running, _ := isRunning(d)
		if !running {
			break
		}
		time.Sleep(500 * time.Millisecond)
	}
	running, _ := isRunning(d)
	if running {
		_ = d.Destroy()
	}
	name, _ := d.GetName()
	uuid, _ := d.GetUUIDString()
	return &pb.Vm{Id: uuid, Name: name, Node: a.node, Status: "stopped"}, nil
}

func (a *Agent) DeleteVm(ctx context.Context, id *pb.VmId) (*pb.Empty, error) {
	d, err := a.lookup(id.Id)
	if err != nil {
		return &pb.Empty{}, nil
	}
	defer d.Free()
	_ = d.Destroy()
	uuid, _ := d.GetUUIDString()
	_ = d.Undefine()
	_ = os.RemoveAll(filepath.Join(a.dataDir, "vms", uuid))
	return &pb.Empty{}, nil
}

func (a *Agent) GetVm(ctx context.Context, id *pb.VmId) (*pb.Vm, error) {
	d, err := a.lookup(id.Id)
	if err != nil {
		return nil, err
	}
	defer d.Free()
	name, _ := d.GetName()
	uuid, _ := d.GetUUIDString()
	status := "stopped"
	if r, _ := isRunning(d); r {
		status = "running"
	}
	return &pb.Vm{Id: uuid, Name: name, Node: a.node, Status: status}, nil
}

func (a *Agent) lookup(id string) (*libvirt.Domain, error) {
	if d, err := a.conn.LookupDomainByUUIDString(id); err == nil {
		return d, nil
	}
	return a.conn.LookupDomainByName(id)
}

func isRunning(d *libvirt.Domain) (bool, error) {
	st, _, err := d.GetState()
	if err != nil {
		return false, err
	}
	return st == libvirt.DOMAIN_RUNNING || st == libvirt.DOMAIN_PAUSED, nil
}

func writeCloudInitISO(dir, iso, userData, metaData string) error {
	if userData == "" {
		userData = "#cloud-config\n"
	}
	if metaData == "" {
		metaData = "instance-id: iid-local01\nlocal-hostname: vm\n"
	}
	ud := filepath.Join(dir, "user-data")
	md := filepath.Join(dir, "meta-data")
	if err := os.WriteFile(ud, []byte(userData), 0o644); err != nil {
		return err
	}
	if err := os.WriteFile(md, []byte(metaData), 0o644); err != nil {
		return err
	}
	return run("genisoimage", "-output", iso, "-volid", "cidata", "-joliet", "-rock", ud, md)
}

func run(name string, args ...string) error {
	cmd := exec.Command(name, args...)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

func randomID() string {
	b := make([]byte, 6)
	_, _ = rand.Read(b)
	return hex.EncodeToString(b)
}
