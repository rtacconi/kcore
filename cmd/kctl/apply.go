package main

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/google/uuid"
	ctrlpb "github.com/kcore/kcore/api/controller"
	"github.com/spf13/cobra"
)

func newApplyCmd() *cobra.Command {
	var (
		filename string
		dryRun   bool
	)

	cmd := &cobra.Command{
		Use:   "apply -f FILENAME",
		Short: "Apply a configuration from a YAML file",
		Long: `Apply a resource definition from a YAML file.

Supported resource kinds:
  VM / VirtualMachine   Create a virtual machine

Examples:
  # Create a VM from a manifest
  kctl apply -f vm.yaml

  # Dry run (show what would be applied)
  kctl apply -f vm.yaml --dry-run`,
		RunE: func(cmd *cobra.Command, args []string) error {
			if filename == "" {
				return fmt.Errorf("filename required: use -f or --filename")
			}

			data, err := os.ReadFile(filename)
			if err != nil {
				return fmt.Errorf("failed to read file: %w", err)
			}

			kind, err := parseManifestKind(data)
			if err != nil {
				return err
			}

			switch {
			case isVMKind(kind):
				return applyVM(cmd, data, dryRun)
			case isNodeInstallKind(kind):
				return applyNodeInstall(cmd, data, dryRun)
			case isNodeNetworkKind(kind):
				return applyNodeNetwork(cmd, data, dryRun)
			case isNodeConfigKind(kind):
				return applyNodeConfig(cmd, data, dryRun)
			default:
				return fmt.Errorf("unsupported resource kind: %q (supported: VM, NodeInstall, NodeNetwork, NodeConfig)", kind)
			}
		},
	}

	cmd.Flags().StringVarP(&filename, "filename", "f", "", "Filename to apply (required)")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "Show what would be applied without applying")
	cmd.MarkFlagRequired("filename")

	return cmd
}

func applyVM(cmd *cobra.Command, data []byte, dryRun bool) error {
	manifest, err := parseVMManifest(data)
	if err != nil {
		return err
	}

	memoryBytes, err := parseMemorySize(manifest.Spec.Memory)
	if err != nil {
		return fmt.Errorf("invalid spec.memory: %w", err)
	}

	enableLogin := true
	if manifest.Spec.EnableKcoreLogin != nil {
		enableLogin = *manifest.Spec.EnableKcoreLogin
	}

	vmID := uuid.New().String()

	var nics []*ctrlpb.Nic
	for _, n := range manifest.Spec.Nics {
		model := n.Model
		if model == "" {
			model = "virtio"
		}
		nics = append(nics, &ctrlpb.Nic{
			Network: n.Network,
			Model:   model,
		})
	}

	spec := &ctrlpb.VmSpec{
		Id:               vmID,
		Name:             manifest.Metadata.Name,
		Cpu:              int32(manifest.Spec.CPU),
		MemoryBytes:      memoryBytes,
		Nics:             nics,
		EnableKcoreLogin: enableLogin,
	}

	imageURI := manifest.Spec.Image
	cloudInit := manifest.Spec.CloudInit

	if dryRun {
		fmt.Printf("Dry run: would create VM from manifest\n\n")
		fmt.Printf("  Name:        %s\n", manifest.Metadata.Name)
		fmt.Printf("  ID:          %s\n", vmID)
		fmt.Printf("  CPU:         %d\n", manifest.Spec.CPU)
		fmt.Printf("  Memory:      %s (%s)\n", manifest.Spec.Memory, formatBytes(memoryBytes))
		if imageURI != "" {
			fmt.Printf("  Image:       %s\n", imageURI)
		}
		fmt.Printf("  Kcore Login: %t\n", enableLogin)
		if len(nics) > 0 {
			for i, n := range nics {
				fmt.Printf("  NIC %d:       %s (%s)\n", i, n.Network, n.Model)
			}
		} else {
			fmt.Printf("  NIC:         default (virtio)\n")
		}
		if cloudInit != "" {
			fmt.Printf("  Cloud-Init:  (custom, %d bytes)\n", len(cloudInit))
		}
		fmt.Printf("\nNo changes made.\n")
		return nil
	}

	root := cmd.Root()
	configPath, _ := root.PersistentFlags().GetString("config")
	controllerAddr, _ := root.PersistentFlags().GetString("controller")
	insecureFlag, _ := root.PersistentFlags().GetBool("insecure")

	ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerAddr, insecureFlag)
	if err != nil {
		return err
	}

	fmt.Printf("Applying VM '%s' via controller %s...\n", manifest.Metadata.Name, ctrlAddr)

	client, err := NewControllerVMClient(ctrlAddr, insecure, certFile, keyFile, caFile)
	if err != nil {
		return fmt.Errorf("failed to connect to controller: %w", err)
	}
	defer client.Close()

	timeout := 30 * time.Second
	if imageURI != "" {
		timeout = 10 * time.Minute
	}
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()

	resp, err := client.CreateVM(ctx, spec, imageURI, cloudInit)
	if err != nil {
		return fmt.Errorf("failed to create VM: %w", err)
	}

	fmt.Printf("\nVM '%s' created successfully\n", manifest.Metadata.Name)
	fmt.Printf("  ID:     %s\n", resp.VmId)
	fmt.Printf("  Node:   %s\n", resp.NodeId)
	fmt.Printf("  State:  %s\n", resp.State.String())
	fmt.Printf("  CPU:    %d cores\n", manifest.Spec.CPU)
	fmt.Printf("  Memory: %s\n", manifest.Spec.Memory)
	if imageURI != "" {
		fmt.Printf("  Image:  %s\n", imageURI)
	}
	return nil
}

func applyNodeInstall(cmd *cobra.Command, data []byte, dryRun bool) error {
	manifest, err := parseNodeInstallManifest(data)
	if err != nil {
		return err
	}

	if dryRun {
		fmt.Printf("Dry run: would install node %q\n", manifest.Metadata.Name)
		fmt.Printf("  Hostname:          %s\n", manifest.Spec.Hostname)
		for _, d := range manifest.Spec.Disks {
			fmt.Printf("  Disk:              %s (%s)\n", d.Device, d.Role)
		}
		fmt.Printf("  Run Controller:    %t\n", manifest.Spec.RunController)
		if manifest.Spec.ControllerAddress != "" {
			fmt.Printf("  Controller Addr:   %s\n", manifest.Spec.ControllerAddress)
		}
		return nil
	}

	root := cmd.Root()
	configPath, _ := root.PersistentFlags().GetString("config")
	controllerAddr, _ := root.PersistentFlags().GetString("controller")
	insecureFlag, _ := root.PersistentFlags().GetBool("insecure")

	ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerAddr, insecureFlag)
	if err != nil {
		return err
	}

	if manifest.Spec.Address != "" {
		ctrlAddr = manifest.Spec.Address
	}

	client, err := NewControllerVMClient(ctrlAddr, insecure, certFile, keyFile, caFile)
	if err != nil {
		return fmt.Errorf("connect to controller: %w", err)
	}
	defer client.Close()

	var disks []*ctrlpb.DiskConfig
	for _, d := range manifest.Spec.Disks {
		disks = append(disks, &ctrlpb.DiskConfig{Device: d.Device, Role: d.Role})
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Minute)
	defer cancel()

	resp, err := client.api.InstallNode(ctx, &ctrlpb.InstallNodeRequest{
		NodeId:            manifest.Metadata.Name,
		Disks:             disks,
		Hostname:          manifest.Spec.Hostname,
		RootPassword:      manifest.Spec.RootPassword,
		SshKeys:           manifest.Spec.SSHKeys,
		RunController:     manifest.Spec.RunController,
		ControllerAddress: manifest.Spec.ControllerAddress,
	})
	if err != nil {
		return fmt.Errorf("install node: %w", err)
	}

	fmt.Printf("Install started: id=%s status=%s\n", resp.InstallId, resp.Status)
	if resp.Message != "" {
		fmt.Printf("  %s\n", resp.Message)
	}
	return nil
}

func applyNodeNetwork(cmd *cobra.Command, data []byte, dryRun bool) error {
	manifest, err := parseNodeNetworkManifest(data)
	if err != nil {
		return err
	}

	if dryRun {
		fmt.Printf("Dry run: would configure network for node %q\n", manifest.Spec.NodeID)
		for _, b := range manifest.Spec.Bridges {
			fmt.Printf("  Bridge: %s ports=%v\n", b.Name, b.MemberPorts)
		}
		return nil
	}

	root := cmd.Root()
	configPath, _ := root.PersistentFlags().GetString("config")
	controllerAddr, _ := root.PersistentFlags().GetString("controller")
	insecureFlag, _ := root.PersistentFlags().GetBool("insecure")

	ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerAddr, insecureFlag)
	if err != nil {
		return err
	}

	client, err := NewControllerVMClient(ctrlAddr, insecure, certFile, keyFile, caFile)
	if err != nil {
		return fmt.Errorf("connect to controller: %w", err)
	}
	defer client.Close()

	req := &ctrlpb.ConfigureNetworkRequest{
		NodeId:     manifest.Spec.NodeID,
		DnsServers: manifest.Spec.DNSServers,
		ApplyNow:   manifest.Spec.ApplyNow,
	}
	for _, b := range manifest.Spec.Bridges {
		req.Bridges = append(req.Bridges, &ctrlpb.BridgeConfig{
			Name: b.Name, MemberPorts: b.MemberPorts,
			IpAddress: b.IPAddress, SubnetMask: b.SubnetMask,
			Gateway: b.Gateway, Dhcp: b.DHCP,
		})
	}
	for _, b := range manifest.Spec.Bonds {
		req.Bonds = append(req.Bonds, &ctrlpb.BondConfig{
			Name: b.Name, MemberPorts: b.MemberPorts, Mode: b.Mode,
			IpAddress: b.IPAddress, SubnetMask: b.SubnetMask, Dhcp: b.DHCP,
		})
	}
	for _, v := range manifest.Spec.Vlans {
		req.Vlans = append(req.Vlans, &ctrlpb.VlanConfig{
			ParentInterface: v.ParentInterface, VlanId: int32(v.VlanID),
			IpAddress: v.IPAddress, SubnetMask: v.SubnetMask, Dhcp: v.DHCP,
		})
	}

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()

	resp, err := client.api.ConfigureNetwork(ctx, req)
	if err != nil {
		return fmt.Errorf("configure network: %w", err)
	}

	if resp.Success {
		fmt.Println("Network configured successfully")
	} else {
		fmt.Printf("Network configuration failed: %s\n", resp.Message)
	}
	return nil
}

func applyNodeConfig(cmd *cobra.Command, data []byte, dryRun bool) error {
	manifest, err := parseNodeConfigManifest(data)
	if err != nil {
		return err
	}

	nixConfig := manifest.Spec.ConfigurationNix
	if nixConfig == "" && manifest.Spec.ConfigFile != "" {
		configData, err := os.ReadFile(manifest.Spec.ConfigFile)
		if err != nil {
			return fmt.Errorf("read config file: %w", err)
		}
		nixConfig = string(configData)
	}
	if nixConfig == "" {
		return fmt.Errorf("spec.configurationNix or spec.configFile is required")
	}

	if dryRun {
		fmt.Printf("Dry run: would push NixOS config to node %q (%d bytes, rebuild=%t)\n",
			manifest.Spec.NodeID, len(nixConfig), manifest.Spec.Rebuild)
		return nil
	}

	root := cmd.Root()
	configPath, _ := root.PersistentFlags().GetString("config")
	controllerAddr, _ := root.PersistentFlags().GetString("controller")
	insecureFlag, _ := root.PersistentFlags().GetBool("insecure")

	ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerAddr, insecureFlag)
	if err != nil {
		return err
	}

	client, err := NewControllerVMClient(ctrlAddr, insecure, certFile, keyFile, caFile)
	if err != nil {
		return fmt.Errorf("connect to controller: %w", err)
	}
	defer client.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()

	resp, err := client.api.UpdateNixConfig(ctx, &ctrlpb.UpdateNixConfigRequest{
		NodeId:           manifest.Spec.NodeID,
		ConfigurationNix: nixConfig,
	})
	if err != nil {
		return fmt.Errorf("push config: %w", err)
	}

	if resp.Success {
		fmt.Println("NixOS config pushed successfully")
		if manifest.Spec.Rebuild {
			fmt.Println("Triggering rebuild...")
			rebuildResp, err := client.api.RebuildNix(ctx, &ctrlpb.RebuildNixRequest{
				NodeId:   manifest.Spec.NodeID,
				Strategy: "switch",
			})
			if err != nil {
				return fmt.Errorf("rebuild: %w", err)
			}
			if rebuildResp.Success {
				fmt.Println("Rebuild completed successfully")
			} else {
				fmt.Printf("Rebuild failed: %s\n", rebuildResp.Message)
			}
		}
	} else {
		fmt.Printf("Failed: %s\n", resp.Message)
	}
	return nil
}
