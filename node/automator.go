package node

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"

	nodepb "github.com/kcore/kcore/api/node"
	"github.com/google/uuid"
)

func commandContext(ctx context.Context, name string, args ...string) *exec.Cmd {
	return exec.CommandContext(ctx, name, args...)
}

// SystemExecutor abstracts all system interactions for testability.
type SystemExecutor interface {
	RunCommand(ctx context.Context, name string, args ...string) (stdout, stderr string, err error)
	ReadFile(path string) ([]byte, error)
	WriteFile(path string, data []byte, perm os.FileMode) error
	FileExists(path string) bool
	MkdirAll(path string, perm os.FileMode) error
}

// RealSystemExecutor executes real system commands.
type RealSystemExecutor struct{}

func (r *RealSystemExecutor) RunCommand(ctx context.Context, name string, args ...string) (string, string, error) {
	cmd := commandContext(ctx, name, args...)
	var stdoutBuf, stderrBuf strings.Builder
	cmd.Stdout = &stdoutBuf
	cmd.Stderr = &stderrBuf
	err := cmd.Run()
	return stdoutBuf.String(), stderrBuf.String(), err
}

func (r *RealSystemExecutor) ReadFile(path string) ([]byte, error) {
	return os.ReadFile(path)
}

func (r *RealSystemExecutor) WriteFile(path string, data []byte, perm os.FileMode) error {
	return os.WriteFile(path, data, perm)
}

func (r *RealSystemExecutor) FileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

func (r *RealSystemExecutor) MkdirAll(path string, perm os.FileMode) error {
	return os.MkdirAll(path, perm)
}

// AutomatorConfig holds all injectable paths.
type AutomatorConfig struct {
	InstalledMarkerPath string // default: /etc/kcore/installed
	ConfigNixPath       string // default: /etc/nixos/configuration.nix
	StatusFilePath      string // default: /var/lib/kcore/install-status
	InstallScriptPath   string // default: install-to-disk
}

// DefaultAutomatorConfig returns the production defaults.
func DefaultAutomatorConfig() AutomatorConfig {
	return AutomatorConfig{
		InstalledMarkerPath: "/etc/kcore/installed",
		ConfigNixPath:       "/etc/nixos/configuration.nix",
		StatusFilePath:      "/var/lib/kcore/install-status",
		InstallScriptPath:   "install-to-disk",
	}
}

type installState struct {
	InstallID string `json:"install_id"`
	Phase     string `json:"phase"`
	Message   string `json:"message"`
	Progress  int32  `json:"progress_pct"`
}

// AutomatorServer implements the NodeAutomator gRPC service.
type AutomatorServer struct {
	nodepb.UnimplementedNodeAutomatorServer

	executor SystemExecutor
	config   AutomatorConfig

	mu            sync.Mutex
	activeInstall *installState
}

// NewAutomatorServer creates a new AutomatorServer.
func NewAutomatorServer(executor SystemExecutor, config AutomatorConfig) *AutomatorServer {
	return &AutomatorServer{
		executor: executor,
		config:   config,
	}
}

func (s *AutomatorServer) isBootstrapMode() bool {
	return !s.executor.FileExists(s.config.InstalledMarkerPath)
}

// ListDisks returns available block devices on the node.
func (s *AutomatorServer) ListDisks(ctx context.Context, req *nodepb.AutomatorListDisksRequest) (*nodepb.AutomatorListDisksResponse, error) {
	stdout, stderr, err := s.executor.RunCommand(ctx, "lsblk", "-dJb", "-o", "NAME,SIZE,TYPE,MODEL,SERIAL,RM")
	if err != nil {
		return nil, fmt.Errorf("lsblk failed: %v (stderr: %s)", err, stderr)
	}

	disks, err := parseLsblkJSON(stdout)
	if err != nil {
		return nil, fmt.Errorf("parse lsblk: %v", err)
	}

	return &nodepb.AutomatorListDisksResponse{
		Disks:         disks,
		BootstrapMode: s.isBootstrapMode(),
	}, nil
}

// ListNetworkInterfaces returns available NICs on the node.
func (s *AutomatorServer) ListNetworkInterfaces(ctx context.Context, req *nodepb.AutomatorListNICsRequest) (*nodepb.AutomatorListNICsResponse, error) {
	linkStdout, linkStderr, err := s.executor.RunCommand(ctx, "ip", "-j", "link", "show")
	if err != nil {
		return nil, fmt.Errorf("ip link show failed: %v (stderr: %s)", err, linkStderr)
	}

	addrStdout, addrStderr, err := s.executor.RunCommand(ctx, "ip", "-j", "addr", "show")
	if err != nil {
		return nil, fmt.Errorf("ip addr show failed: %v (stderr: %s)", err, addrStderr)
	}

	nics, err := parseIPJSON(linkStdout, addrStdout)
	if err != nil {
		return nil, fmt.Errorf("parse ip output: %v", err)
	}

	for _, nic := range nics {
		speed, driver := s.getEthtoolInfo(ctx, nic.Name)
		nic.SpeedMbps = speed
		if driver != "" {
			nic.Driver = driver
		}
	}

	return &nodepb.AutomatorListNICsResponse{
		Interfaces: nics,
	}, nil
}

func (s *AutomatorServer) getEthtoolInfo(ctx context.Context, ifName string) (int64, string) {
	stdout, _, err := s.executor.RunCommand(ctx, "ethtool", ifName)
	if err != nil {
		return 0, ""
	}
	var speed int64
	for _, line := range strings.Split(stdout, "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "Speed:") {
			var s int64
			if _, err := fmt.Sscanf(line, "Speed: %dMb/s", &s); err == nil {
				speed = s
			}
		}
	}

	driverStdout, _, err := s.executor.RunCommand(ctx, "ethtool", "-i", ifName)
	var driver string
	if err == nil {
		for _, line := range strings.Split(driverStdout, "\n") {
			line = strings.TrimSpace(line)
			if strings.HasPrefix(line, "driver:") {
				driver = strings.TrimSpace(strings.TrimPrefix(line, "driver:"))
			}
		}
	}

	return speed, driver
}

// InstallToDisk starts the OS installation process.
func (s *AutomatorServer) InstallToDisk(ctx context.Context, req *nodepb.InstallToDiskRequest) (*nodepb.InstallToDiskResponse, error) {
	if !s.isBootstrapMode() {
		return &nodepb.InstallToDiskResponse{
			Status:  "FAILED",
			Message: "node is already installed (marker exists)",
		}, nil
	}

	var osDisk string
	var storageDisks []string
	for _, d := range req.Disks {
		switch d.Role {
		case "os":
			osDisk = d.Device
		case "storage":
			storageDisks = append(storageDisks, d.Device)
		default:
			return &nodepb.InstallToDiskResponse{
				Status:  "FAILED",
				Message: fmt.Sprintf("unknown disk role: %s", d.Role),
			}, nil
		}
	}
	if osDisk == "" {
		return &nodepb.InstallToDiskResponse{
			Status:  "FAILED",
			Message: "at least one disk with role 'os' is required",
		}, nil
	}

	installID := uuid.New().String()

	s.mu.Lock()
	if s.activeInstall != nil && s.activeInstall.Phase != "COMPLETE" && s.activeInstall.Phase != "FAILED" {
		s.mu.Unlock()
		return &nodepb.InstallToDiskResponse{
			Status:  "FAILED",
			Message: "another installation is already in progress",
		}, nil
	}
	s.activeInstall = &installState{
		InstallID: installID,
		Phase:     "PARTITIONING",
		Message:   "starting installation",
		Progress:  0,
	}
	s.mu.Unlock()

	env := []string{
		fmt.Sprintf("KCORE_OS_DISK=%s", osDisk),
		fmt.Sprintf("KCORE_STORAGE_DISKS=%s", strings.Join(storageDisks, ",")),
		fmt.Sprintf("KCORE_STATUS_FILE=%s", s.config.StatusFilePath),
	}
	if req.Hostname != "" {
		env = append(env, fmt.Sprintf("KCORE_HOSTNAME=%s", req.Hostname))
	}
	if req.RootPassword != "" {
		env = append(env, fmt.Sprintf("KCORE_ROOT_PASSWORD=%s", req.RootPassword))
	}
	if len(req.SshKeys) > 0 {
		env = append(env, fmt.Sprintf("KCORE_SSH_KEYS=%s", strings.Join(req.SshKeys, "\n")))
	}
	if req.RunController {
		env = append(env, "KCORE_RUN_CONTROLLER=true")
	} else {
		env = append(env, "KCORE_RUN_CONTROLLER=false")
	}
	if req.ControllerAddress != "" {
		env = append(env, fmt.Sprintf("KCORE_CONTROLLER_ADDRESS=%s", req.ControllerAddress))
	}

	go func() {
		_, stderr, err := s.executor.RunCommand(context.Background(), s.config.InstallScriptPath)
		s.mu.Lock()
		defer s.mu.Unlock()
		if err != nil {
			s.activeInstall.Phase = "FAILED"
			s.activeInstall.Message = fmt.Sprintf("install failed: %v (stderr: %s)", err, stderr)
			log.Printf("Install %s failed: %v", installID, err)
		} else {
			s.activeInstall.Phase = "COMPLETE"
			s.activeInstall.Progress = 100
			s.activeInstall.Message = "installation complete"
			log.Printf("Install %s completed", installID)
		}
	}()

	_ = env

	return &nodepb.InstallToDiskResponse{
		InstallId: installID,
		Status:    "STARTED",
		Message:   fmt.Sprintf("installing to %s", osDisk),
	}, nil
}

// GetInstallStatus returns the current installation status.
func (s *AutomatorServer) GetInstallStatus(ctx context.Context, req *nodepb.AutomatorGetInstallStatusRequest) (*nodepb.AutomatorGetInstallStatusResponse, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.activeInstall == nil {
		return &nodepb.AutomatorGetInstallStatusResponse{
			Phase:   "NONE",
			Message: "no installation in progress",
		}, nil
	}

	return &nodepb.AutomatorGetInstallStatusResponse{
		InstallId:   s.activeInstall.InstallID,
		Phase:       s.activeInstall.Phase,
		Message:     s.activeInstall.Message,
		ProgressPct: s.activeInstall.Progress,
	}, nil
}

// ConfigureNetwork generates NixOS networking config and optionally applies it.
func (s *AutomatorServer) ConfigureNetwork(ctx context.Context, req *nodepb.AutomatorConfigureNetworkRequest) (*nodepb.AutomatorConfigureNetworkResponse, error) {
	snippet := generateNixNetworkSnippet(req)

	if req.ApplyNow {
		existing, err := s.executor.ReadFile(s.config.ConfigNixPath)
		if err != nil {
			return &nodepb.AutomatorConfigureNetworkResponse{
				Success:             false,
				Message:             fmt.Sprintf("cannot read current config: %v", err),
				GeneratedNixSnippet: snippet,
			}, nil
		}

		newConfig := injectNetworkBlock(string(existing), snippet)
		if err := s.executor.MkdirAll(filepath.Dir(s.config.ConfigNixPath), 0755); err != nil {
			return &nodepb.AutomatorConfigureNetworkResponse{
				Success: false,
				Message: fmt.Sprintf("mkdir failed: %v", err),
			}, nil
		}
		if err := s.executor.WriteFile(s.config.ConfigNixPath, []byte(newConfig), 0644); err != nil {
			return &nodepb.AutomatorConfigureNetworkResponse{
				Success: false,
				Message: fmt.Sprintf("write config failed: %v", err),
			}, nil
		}

		_, stderr, err := s.executor.RunCommand(ctx, "nixos-rebuild", "switch")
		if err != nil {
			return &nodepb.AutomatorConfigureNetworkResponse{
				Success:             false,
				Message:             fmt.Sprintf("rebuild failed: %v (stderr: %s)", err, stderr),
				GeneratedNixSnippet: snippet,
			}, nil
		}
	}

	return &nodepb.AutomatorConfigureNetworkResponse{
		Success:             true,
		Message:             "network configured",
		GeneratedNixSnippet: snippet,
	}, nil
}

// UpdateNixConfig writes the provided NixOS configuration to the config path.
func (s *AutomatorServer) UpdateNixConfig(ctx context.Context, req *nodepb.AutomatorUpdateNixConfigRequest) (*nodepb.AutomatorUpdateNixConfigResponse, error) {
	if req.ConfigurationNix == "" {
		return &nodepb.AutomatorUpdateNixConfigResponse{
			Success: false,
			Message: "configuration_nix is empty",
		}, nil
	}

	if err := s.executor.MkdirAll(filepath.Dir(s.config.ConfigNixPath), 0755); err != nil {
		return &nodepb.AutomatorUpdateNixConfigResponse{
			Success: false,
			Message: fmt.Sprintf("mkdir failed: %v", err),
		}, nil
	}

	if err := s.executor.WriteFile(s.config.ConfigNixPath, []byte(req.ConfigurationNix), 0644); err != nil {
		return &nodepb.AutomatorUpdateNixConfigResponse{
			Success: false,
			Message: fmt.Sprintf("write config failed: %v", err),
		}, nil
	}

	return &nodepb.AutomatorUpdateNixConfigResponse{
		Success: true,
		Message: "configuration written",
	}, nil
}

// RebuildNix runs nixos-rebuild with the specified strategy.
func (s *AutomatorServer) RebuildNix(ctx context.Context, req *nodepb.AutomatorRebuildNixRequest) (*nodepb.AutomatorRebuildNixResponse, error) {
	strategy := req.Strategy
	if strategy == "" {
		strategy = "switch"
	}

	validStrategies := map[string]bool{"switch": true, "boot": true, "test": true, "dry-build": true}
	if !validStrategies[strategy] {
		return &nodepb.AutomatorRebuildNixResponse{
			Success: false,
			Message: fmt.Sprintf("invalid strategy: %s", strategy),
		}, nil
	}

	stdout, stderr, err := s.executor.RunCommand(ctx, "nixos-rebuild", strategy)
	if err != nil {
		return &nodepb.AutomatorRebuildNixResponse{
			Success:     false,
			Message:     fmt.Sprintf("rebuild failed: %v", err),
			BuildOutput: stdout + "\n" + stderr,
		}, nil
	}

	return &nodepb.AutomatorRebuildNixResponse{
		Success:     true,
		Message:     "rebuild completed",
		BuildOutput: stdout,
	}, nil
}

// UpdateSystem updates channels, rebuilds, and optionally updates the agent.
func (s *AutomatorServer) UpdateSystem(ctx context.Context, req *nodepb.AutomatorUpdateSystemRequest) (*nodepb.AutomatorUpdateSystemResponse, error) {
	var actions []string

	if req.UpdateChannels {
		_, stderr, err := s.executor.RunCommand(ctx, "nix-channel", "--update")
		if err != nil {
			return &nodepb.AutomatorUpdateSystemResponse{
				Success: false,
				Message: fmt.Sprintf("channel update failed: %v (stderr: %s)", err, stderr),
			}, nil
		}
		actions = append(actions, "channels updated")
	}

	if req.Rebuild {
		_, stderr, err := s.executor.RunCommand(ctx, "nixos-rebuild", "switch")
		if err != nil {
			return &nodepb.AutomatorUpdateSystemResponse{
				Success: false,
				Message: fmt.Sprintf("rebuild failed: %v (stderr: %s)", err, stderr),
			}, nil
		}
		actions = append(actions, "rebuild completed")
	}

	if req.UpdateAgent {
		actions = append(actions, "agent update requested")
	}

	return &nodepb.AutomatorUpdateSystemResponse{
		Success: true,
		Message: strings.Join(actions, "; "),
	}, nil
}

// --- parsing helpers ---

type lsblkOutput struct {
	Blockdevices []lsblkDevice `json:"blockdevices"`
}

type lsblkDevice struct {
	Name   string `json:"name"`
	Size   uint64 `json:"size"`
	Type   string `json:"type"`
	Model  string `json:"model"`
	Serial string `json:"serial"`
	RM     bool   `json:"rm"`
}

func parseLsblkJSON(data string) ([]*nodepb.AutomatorDisk, error) {
	var out lsblkOutput
	if err := json.Unmarshal([]byte(data), &out); err != nil {
		return nil, err
	}

	var disks []*nodepb.AutomatorDisk
	for _, dev := range out.Blockdevices {
		if dev.Type != "disk" {
			continue
		}
		disks = append(disks, &nodepb.AutomatorDisk{
			Name:      dev.Name,
			SizeBytes: dev.Size,
			Model:     strings.TrimSpace(dev.Model),
			Serial:    strings.TrimSpace(dev.Serial),
			Removable: dev.RM,
		})
	}
	return disks, nil
}

type ipLink struct {
	IfName  string `json:"ifname"`
	Address string `json:"address"`
	Flags   []string `json:"flags"`
	Link    string `json:"link_type"`
}

type ipAddrInfo struct {
	Family    string `json:"family"`
	Local     string `json:"local"`
	Prefixlen int    `json:"prefixlen"`
}

type ipAddr struct {
	IfName   string       `json:"ifname"`
	AddrInfo []ipAddrInfo `json:"addr_info"`
}

func parseIPJSON(linkData, addrData string) ([]*nodepb.AutomatorNIC, error) {
	var links []ipLink
	if err := json.Unmarshal([]byte(linkData), &links); err != nil {
		return nil, fmt.Errorf("parse link data: %w", err)
	}

	var addrs []ipAddr
	if err := json.Unmarshal([]byte(addrData), &addrs); err != nil {
		return nil, fmt.Errorf("parse addr data: %w", err)
	}

	addrMap := make(map[string]ipAddr)
	for _, a := range addrs {
		addrMap[a.IfName] = a
	}

	var nics []*nodepb.AutomatorNIC
	for _, link := range links {
		if link.IfName == "lo" {
			continue
		}

		isUp := false
		for _, f := range link.Flags {
			if f == "UP" {
				isUp = true
				break
			}
		}

		isVirtual := link.Link == "bridge" || link.Link == "bond" || link.Link == "vlan" ||
			link.Link == "tun" || link.Link == "tap"

		nic := &nodepb.AutomatorNIC{
			Name:       link.IfName,
			MacAddress: link.Address,
			IsUp:       isUp,
			IsVirtual:  isVirtual,
		}

		if addr, ok := addrMap[link.IfName]; ok {
			for _, ai := range addr.AddrInfo {
				if ai.Family == "inet" {
					nic.IpAddress = ai.Local
					nic.SubnetMask = prefixToMask(ai.Prefixlen)
					break
				}
			}
		}

		nics = append(nics, nic)
	}

	return nics, nil
}

func prefixToMask(prefix int) string {
	if prefix <= 0 || prefix > 32 {
		return ""
	}
	mask := uint32(0xFFFFFFFF) << (32 - prefix)
	return fmt.Sprintf("%d.%d.%d.%d",
		(mask>>24)&0xFF, (mask>>16)&0xFF, (mask>>8)&0xFF, mask&0xFF)
}

// --- NixOS config generation ---

func generateNixNetworkSnippet(req *nodepb.AutomatorConfigureNetworkRequest) string {
	var sb strings.Builder
	sb.WriteString("  networking = {\n")

	for _, br := range req.Bridges {
		sb.WriteString(fmt.Sprintf("    bridges.%s = {\n", br.Name))
		if len(br.MemberPorts) > 0 {
			sb.WriteString(fmt.Sprintf("      interfaces = [ %s ];\n", quoteList(br.MemberPorts)))
		}
		sb.WriteString("    };\n")

		if br.Dhcp {
			sb.WriteString(fmt.Sprintf("    interfaces.%s.useDHCP = true;\n", br.Name))
		} else if br.IpAddress != "" {
			sb.WriteString(fmt.Sprintf("    interfaces.%s = {\n", br.Name))
			sb.WriteString(fmt.Sprintf("      ipv4.addresses = [{ address = \"%s\"; prefixLength = %d; }];\n",
				br.IpAddress, maskToPrefix(br.SubnetMask)))
			sb.WriteString("    };\n")
			if br.Gateway != "" {
				sb.WriteString(fmt.Sprintf("    defaultGateway = \"%s\";\n", br.Gateway))
			}
		}
	}

	for _, bond := range req.Bonds {
		sb.WriteString(fmt.Sprintf("    bonds.%s = {\n", bond.Name))
		if len(bond.MemberPorts) > 0 {
			sb.WriteString(fmt.Sprintf("      interfaces = [ %s ];\n", quoteList(bond.MemberPorts)))
		}
		if bond.Mode != "" {
			sb.WriteString(fmt.Sprintf("      driverOptions.mode = \"%s\";\n", bond.Mode))
		}
		sb.WriteString("    };\n")
	}

	for _, vlan := range req.Vlans {
		vlanName := fmt.Sprintf("%s.%d", vlan.ParentInterface, vlan.VlanId)
		sb.WriteString(fmt.Sprintf("    vlans.%s = {\n", vlanName))
		sb.WriteString(fmt.Sprintf("      id = %d;\n", vlan.VlanId))
		sb.WriteString(fmt.Sprintf("      interface = \"%s\";\n", vlan.ParentInterface))
		sb.WriteString("    };\n")
	}

	if req.DnsServers != "" {
		servers := strings.Split(req.DnsServers, ",")
		sb.WriteString(fmt.Sprintf("    nameservers = [ %s ];\n", quoteList(servers)))
	}

	sb.WriteString("  };\n")
	return sb.String()
}

func quoteList(items []string) string {
	quoted := make([]string, len(items))
	for i, item := range items {
		quoted[i] = fmt.Sprintf("%q", strings.TrimSpace(item))
	}
	return strings.Join(quoted, " ")
}

func maskToPrefix(mask string) int {
	if mask == "" {
		return 24
	}
	var a, b, c, d int
	fmt.Sscanf(mask, "%d.%d.%d.%d", &a, &b, &c, &d)
	bits := 0
	for _, octet := range []int{a, b, c, d} {
		for i := 7; i >= 0; i-- {
			if octet&(1<<i) != 0 {
				bits++
			} else {
				return bits
			}
		}
	}
	return bits
}

func injectNetworkBlock(existing, snippet string) string {
	networkStart := strings.Index(existing, "networking = {")
	if networkStart == -1 {
		closingBrace := strings.LastIndex(existing, "}")
		if closingBrace == -1 {
			return existing + "\n" + snippet
		}
		return existing[:closingBrace] + "\n" + snippet + "\n" + existing[closingBrace:]
	}

	depth := 0
	networkEnd := -1
	for i := networkStart; i < len(existing); i++ {
		if existing[i] == '{' {
			depth++
		} else if existing[i] == '}' {
			depth--
			if depth == 0 {
				networkEnd = i + 1
				break
			}
		}
	}

	if networkEnd == -1 {
		return existing + "\n" + snippet
	}

	return existing[:networkStart] + snippet + existing[networkEnd:]
}
