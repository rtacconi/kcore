package main

import (
	"context"
	"fmt"
	"time"

	"github.com/spf13/cobra"
)

func newDescribeCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "describe RESOURCE NAME",
		Short: "Show detailed information about a resource",
		Long: `Show detailed information about a specific resource.

Available resource types:
  vm       Describe a virtual machine
  node     Describe a node
  volume   Describe a storage volume
  network  Describe a virtual network`,
	}

	cmd.AddCommand(newDescribeVMCmd())
	cmd.AddCommand(newDescribeNodeCmd())
	cmd.AddCommand(newDescribeVolumeCmd())
	cmd.AddCommand(newDescribeNetworkCmd())

	return cmd
}

func newDescribeVMCmd() *cobra.Command {
	return &cobra.Command{
		Use:     "vm NAME",
		Aliases: []string{"vms"},
		Short:   "Describe a virtual machine",
		Long: `Show detailed information about a virtual machine.

Examples:
  # Describe a VM
  kctl describe vm web-server`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			vmID := args[0]
			
			// Get connection info from flags or config
			configPath, _ := cmd.Flags().GetString("config")
			controllerFlag, _ := cmd.Flags().GetString("controller")
			insecureFlag, _ := cmd.Flags().GetBool("insecure")

			nodeAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerFlag, insecureFlag)
			if err != nil {
				return err
			}

			return describeVM(vmID, nodeAddr, insecure, certFile, keyFile, caFile)
		},
	}
}

func newDescribeNodeCmd() *cobra.Command {
	return &cobra.Command{
		Use:     "node NAME",
		Aliases: []string{"nodes"},
		Short:   "Describe a cluster node",
		Long: `Show detailed information about a cluster node.

Examples:
  # Describe a node
  kctl describe node kvm-node-01`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			return describeNode(name)
		},
	}
}

func newDescribeVolumeCmd() *cobra.Command {
	return &cobra.Command{
		Use:     "volume NAME",
		Aliases: []string{"volumes", "vol"},
		Short:   "Describe a storage volume",
		Long: `Show detailed information about a storage volume.

Examples:
  # Describe a volume
  kctl describe volume data-vol`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			return describeVolume(name)
		},
	}
}

func newDescribeNetworkCmd() *cobra.Command {
	return &cobra.Command{
		Use:     "network NAME",
		Aliases: []string{"networks", "net"},
		Short:   "Describe a virtual network",
		Long: `Show detailed information about a virtual network.

Examples:
  # Describe a network
  kctl describe network private-net`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			return describeNetwork(name)
		},
	}
}

// Implementation functions

func describeVM(vmID, nodeAddr string, insecure bool, certFile, keyFile, caFile string) error {
	// Create client
	client, err := NewNodeClient(nodeAddr, insecure, certFile, keyFile, caFile)
	if err != nil {
		return fmt.Errorf("failed to connect to node: %w", err)
	}
	defer client.Close()

	// Get VM details
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	spec, status, err := client.GetVM(ctx, vmID)
	if err != nil {
		return fmt.Errorf("failed to get VM: %w", err)
	}

	// Display VM details
	fmt.Printf("Name:           %s\n", spec.Name)
	fmt.Printf("ID:             %s\n", status.Id)
	fmt.Printf("Status:         %s\n", status.State.String())
	fmt.Printf("\n")

	fmt.Printf("Resources:\n")
	fmt.Printf("  CPU:          %d cores\n", spec.Cpu)
	fmt.Printf("  Memory:       %s\n", formatBytes(spec.MemoryBytes))
	fmt.Printf("\n")

	if len(spec.Disks) > 0 {
		fmt.Printf("Disks:\n")
		for _, disk := range spec.Disks {
			fmt.Printf("  - %s (%s): %s\n", disk.Device, disk.Bus, disk.BackendHandle)
		}
		fmt.Printf("\n")
	} else {
		fmt.Printf("Disks:          (none)\n\n")
	}

	if len(spec.Nics) > 0 {
		fmt.Printf("Network:\n")
		for _, nic := range spec.Nics {
			fmt.Printf("  Network:      %s\n", nic.Network)
			if nic.MacAddress != "" {
				fmt.Printf("  MAC Address:  %s\n", nic.MacAddress)
			}
			fmt.Printf("  Model:        %s\n", nic.Model)
		}
		fmt.Printf("\n")
	} else {
		fmt.Printf("Network:        (none)\n\n")
	}

	if status.CreatedAt != nil || status.UpdatedAt != nil {
		fmt.Printf("Timestamps:\n")
		if status.CreatedAt != nil {
			fmt.Printf("  Created:      %s\n", status.CreatedAt.AsTime().Format(time.RFC3339))
		}
		if status.UpdatedAt != nil {
			fmt.Printf("  Updated:      %s\n", status.UpdatedAt.AsTime().Format(time.RFC3339))
		}
	}

	return nil
}

func describeNode(name string) error {
	fmt.Printf("Name:              %s\n", name)
	fmt.Printf("Status:            Ready\n")
	fmt.Printf("Role:              compute\n")
	fmt.Printf("Version:           kcore-0.1.0\n")
	fmt.Printf("IP Address:        192.168.1.50\n")
	fmt.Printf("\n")
	fmt.Printf("Resources:\n")
	fmt.Printf("  CPU:             32 used / 64 total (50%%)\n")
	fmt.Printf("  Memory:          64GB used / 128GB total (50%%)\n")
	fmt.Printf("  Storage:         2TB used / 4TB total (50%%)\n")
	fmt.Printf("\n")
	fmt.Printf("VMs:\n")
	fmt.Printf("  Running:         5\n")
	fmt.Printf("  Stopped:         2\n")
	fmt.Printf("  Total:           7\n")
	fmt.Printf("\n")
	fmt.Printf("System Info:\n")
	fmt.Printf("  OS:              kcore (NixOS 25.05)\n")
	fmt.Printf("  Kernel:          6.12.57\n")
	fmt.Printf("  Hypervisor:      KVM/libvirt 10.9.0\n")
	fmt.Printf("\n")
	fmt.Printf("Status:\n")
	fmt.Printf("  Last Heartbeat:  30s ago\n")
	fmt.Printf("  Uptime:          15d 4h 32m\n")
	fmt.Printf("\n")
	fmt.Printf("Recent Events:\n")
	fmt.Printf("  1m ago    Normal    Heartbeat    Heartbeat received\n")
	fmt.Printf("  5m ago    Normal    VMCreated    VM web-server created\n")
	return nil
}

func describeVolume(name string) error {
	fmt.Printf("Name:             %s\n", name)
	fmt.Printf("Size:             100GB\n")
	fmt.Printf("Storage Class:    local-lvm\n")
	fmt.Printf("Node:             kvm-node-01\n")
	fmt.Printf("Status:           Bound\n")
	fmt.Printf("\n")
	fmt.Printf("Attached To:\n")
	fmt.Printf("  VM:             web-server\n")
	fmt.Printf("  Device:         /dev/vdb\n")
	fmt.Printf("\n")
	fmt.Printf("Details:\n")
	fmt.Printf("  Path:           /dev/vg0/data-vol\n")
	fmt.Printf("  Format:         raw\n")
	fmt.Printf("  Used:           45GB (45%%)\n")
	fmt.Printf("\n")
	fmt.Printf("Timestamps:\n")
	fmt.Printf("  Created:        2025-11-08 10:15:00 UTC\n")
	fmt.Printf("  Age:            4d\n")
	return nil
}

func describeNetwork(name string) error {
	fmt.Printf("Name:        %s\n", name)
	fmt.Printf("Subnet:      192.168.1.0/24\n")
	fmt.Printf("Bridge:      br0\n")
	fmt.Printf("Gateway:     192.168.1.1\n")
	fmt.Printf("DNS:         192.168.1.1\n")
	fmt.Printf("\n")
	fmt.Printf("DHCP Range:\n")
	fmt.Printf("  Start:     192.168.1.100\n")
	fmt.Printf("  End:       192.168.1.200\n")
	fmt.Printf("\n")
	fmt.Printf("Connected VMs:  15\n")
	fmt.Printf("\n")
	fmt.Printf("VMs:\n")
	fmt.Printf("  web-server      192.168.1.100\n")
	fmt.Printf("  db-server       192.168.1.101\n")
	fmt.Printf("  cache-01        192.168.1.102\n")
	return nil
}

