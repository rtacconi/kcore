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
		Long: `Show detailed information about a virtual machine (via the controller).

Examples:
  # Describe a VM
  kctl describe vm web-server`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			vmID := args[0]

			configPath, _ := cmd.Flags().GetString("config")
			controllerFlag, _ := cmd.Flags().GetString("controller")
			insecureFlag, _ := cmd.Flags().GetBool("insecure")

			ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerFlag, insecureFlag)
			if err != nil {
				return err
			}

			return describeVM(vmID, ctrlAddr, insecure, certFile, keyFile, caFile)
		},
	}
}

func newDescribeNodeCmd() *cobra.Command {
	return &cobra.Command{
		Use:     "node NAME",
		Aliases: []string{"nodes"},
		Short:   "Describe a cluster node",
		Args:    cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			configPath, _ := cmd.Flags().GetString("config")
			controllerFlag, _ := cmd.Flags().GetString("controller")
			insecureFlag, _ := cmd.Flags().GetBool("insecure")

			ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerFlag, insecureFlag)
			if err != nil {
				return err
			}

			return describeNode(args[0], ctrlAddr, insecure, certFile, keyFile, caFile)
		},
	}
}

func newDescribeVolumeCmd() *cobra.Command {
	return &cobra.Command{
		Use:     "volume NAME",
		Aliases: []string{"volumes", "vol"},
		Short:   "Describe a storage volume",
		Args:    cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			fmt.Printf("Volume '%s' not found (volumes are a future feature)\n", args[0])
			return nil
		},
	}
}

func newDescribeNetworkCmd() *cobra.Command {
	return &cobra.Command{
		Use:     "network NAME",
		Aliases: []string{"networks", "net"},
		Short:   "Describe a virtual network",
		Args:    cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			fmt.Printf("Network '%s' not found (networks are a future feature)\n", args[0])
			return nil
		},
	}
}

// --- VM describe (via controller) ------------------------------------------

func describeVM(vmID, ctrlAddr string, insecure bool, certFile, keyFile, caFile string) error {
	client, err := NewControllerVMClient(ctrlAddr, insecure, certFile, keyFile, caFile)
	if err != nil {
		return fmt.Errorf("failed to connect to controller: %w", err)
	}
	defer client.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	resp, err := client.GetVM(ctx, vmID)
	if err != nil {
		return fmt.Errorf("failed to get VM: %w", err)
	}

	if resp.Spec != nil {
		fmt.Printf("Name:           %s\n", resp.Spec.Name)
	}
	if resp.Status != nil {
		fmt.Printf("ID:             %s\n", resp.Status.Id)
		fmt.Printf("Status:         %s\n", resp.Status.State.String())
	}
	fmt.Printf("Node:           %s\n", resp.NodeId)
	fmt.Printf("\n")

	if resp.Spec != nil {
		fmt.Printf("Resources:\n")
		fmt.Printf("  CPU:          %d cores\n", resp.Spec.Cpu)
		fmt.Printf("  Memory:       %s\n", formatBytes(resp.Spec.MemoryBytes))
		fmt.Printf("\n")

		if len(resp.Spec.Disks) > 0 {
			fmt.Printf("Disks:\n")
			for _, disk := range resp.Spec.Disks {
				fmt.Printf("  - %s (%s): %s\n", disk.Device, disk.Bus, disk.BackendHandle)
			}
			fmt.Printf("\n")
		} else {
			fmt.Printf("Disks:          (none)\n\n")
		}

		if len(resp.Spec.Nics) > 0 {
			fmt.Printf("Network:\n")
			for _, nic := range resp.Spec.Nics {
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
	}

	if resp.Status != nil {
		if resp.Status.CreatedAt != nil || resp.Status.UpdatedAt != nil {
			fmt.Printf("Timestamps:\n")
			if resp.Status.CreatedAt != nil {
				fmt.Printf("  Created:      %s\n", resp.Status.CreatedAt.AsTime().Format(time.RFC3339))
			}
			if resp.Status.UpdatedAt != nil {
				fmt.Printf("  Updated:      %s\n", resp.Status.UpdatedAt.AsTime().Format(time.RFC3339))
			}
		}
	}

	return nil
}

// --- Node describe (via controller) ----------------------------------------

func describeNode(name, ctrlAddr string, insecure bool, certFile, keyFile, caFile string) error {
	client, err := NewControllerVMClient(ctrlAddr, insecure, certFile, keyFile, caFile)
	if err != nil {
		return fmt.Errorf("failed to connect to controller: %w", err)
	}
	defer client.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	node, err := client.GetNode(ctx, name)
	if err != nil {
		return fmt.Errorf("failed to get node: %w", err)
	}

	fmt.Printf("Node ID:        %s\n", node.NodeId)
	fmt.Printf("Hostname:       %s\n", node.Hostname)
	fmt.Printf("Address:        %s\n", node.Address)
	fmt.Printf("Status:         %s\n", node.Status)
	fmt.Printf("\n")
	fmt.Printf("Resources:\n")
	if node.Capacity != nil {
		fmt.Printf("  CPU:          %d cores\n", node.Capacity.CpuCores)
		fmt.Printf("  Memory:       %s\n", formatBytes(node.Capacity.MemoryBytes))
	}
	if node.Usage != nil {
		fmt.Printf("  CPU Used:     %d cores\n", node.Usage.CpuCoresUsed)
		fmt.Printf("  Memory Used:  %s\n", formatBytes(node.Usage.MemoryBytesUsed))
	}
	fmt.Printf("\n")
	if node.LastHeartbeat != nil {
		fmt.Printf("Last Heartbeat: %s\n", node.LastHeartbeat.AsTime().Format(time.RFC3339))
	}

	return nil
}
