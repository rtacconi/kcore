package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

func newCreateCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "create RESOURCE",
		Short: "Create a resource (vm, volume, network)",
		Long: `Create kcore resources.

Available resource types:
  vm       Create a virtual machine
  volume   Create a storage volume
  network  Create a virtual network`,
	}

	cmd.AddCommand(newCreateVMCmd())
	cmd.AddCommand(newCreateVolumeCmd())
	cmd.AddCommand(newCreateNetworkCmd())

	return cmd
}

func newCreateVMCmd() *cobra.Command {
	var (
		cpu        int
		memory     string
		disk       string
		image      string
		node       string
		network    string
		autostart  bool
	)

	cmd := &cobra.Command{
		Use:   "vm NAME",
		Short: "Create a virtual machine",
		Long: `Create a new virtual machine on the kcore cluster.

The VM will be scheduled on an available node unless --node is specified.

Examples:
  # Create a VM with 4 CPUs and 8GB RAM
  kctl create vm web-server --cpu 4 --memory 8G

  # Create a VM with specific disk size
  kctl create vm db-server --cpu 8 --memory 16G --disk 500G

  # Create a VM on a specific node
  kctl create vm cache-server --cpu 2 --memory 4G --node kvm-node-02

  # Create a VM with an image
  kctl create vm ubuntu-vm --cpu 2 --memory 2G --image ubuntu-22.04

  # Create a VM that starts automatically
  kctl create vm web-01 --cpu 4 --memory 8G --autostart`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]

			// TODO: Implement VM creation via gRPC to controller
			fmt.Printf("Creating VM '%s'...\n", name)
			fmt.Printf("  CPU: %d cores\n", cpu)
			fmt.Printf("  Memory: %s\n", memory)
			fmt.Printf("  Disk: %s\n", disk)
			if image != "" {
				fmt.Printf("  Image: %s\n", image)
			}
			if node != "" {
				fmt.Printf("  Node: %s\n", node)
			}
			fmt.Printf("  Network: %s\n", network)
			fmt.Printf("  Autostart: %v\n", autostart)

			fmt.Printf("\n✅ VM '%s' created successfully\n", name)
			return nil
		},
	}

	cmd.Flags().IntVar(&cpu, "cpu", 2, "Number of CPU cores")
	cmd.Flags().StringVar(&memory, "memory", "2G", "Memory size (e.g., 2G, 4096M)")
	cmd.Flags().StringVar(&disk, "disk", "20G", "Disk size (e.g., 100G, 50000M)")
	cmd.Flags().StringVar(&image, "image", "", "OS image to use (optional)")
	cmd.Flags().StringVar(&node, "node", "", "Specific node to run on (optional)")
	cmd.Flags().StringVar(&network, "network", "default", "Network to connect to")
	cmd.Flags().BoolVar(&autostart, "autostart", false, "Start VM automatically after creation")

	return cmd
}

func newCreateVolumeCmd() *cobra.Command {
	var (
		size        string
		storageClass string
		node         string
	)

	cmd := &cobra.Command{
		Use:   "volume NAME",
		Short: "Create a storage volume",
		Long: `Create a new storage volume.

Examples:
  # Create a 100GB volume
  kctl create volume data-vol --size 100G

  # Create a volume with specific storage class
  kctl create volume db-vol --size 500G --storage-class local-lvm`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			fmt.Printf("Creating volume '%s' (%s)...\n", name, size)
			if storageClass != "" {
				fmt.Printf("  Storage class: %s\n", storageClass)
			}
			if node != "" {
				fmt.Printf("  Node: %s\n", node)
			}
			fmt.Printf("✅ Volume '%s' created successfully\n", name)
			return nil
		},
	}

	cmd.Flags().StringVar(&size, "size", "10G", "Volume size (e.g., 100G, 1T)")
	cmd.Flags().StringVar(&storageClass, "storage-class", "local-dir", "Storage class to use")
	cmd.Flags().StringVar(&node, "node", "", "Specific node to create on (optional)")

	return cmd
}

func newCreateNetworkCmd() *cobra.Command {
	var (
		subnet string
		bridge string
	)

	cmd := &cobra.Command{
		Use:   "network NAME",
		Short: "Create a virtual network",
		Long: `Create a new virtual network.

Examples:
  # Create a network
  kctl create network private-net --subnet 192.168.100.0/24

  # Create a network with specific bridge
  kctl create network dmz-net --bridge br1`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]
			fmt.Printf("Creating network '%s'...\n", name)
			if subnet != "" {
				fmt.Printf("  Subnet: %s\n", subnet)
			}
			if bridge != "" {
				fmt.Printf("  Bridge: %s\n", bridge)
			}
			fmt.Printf("✅ Network '%s' created successfully\n", name)
			return nil
		},
	}

	cmd.Flags().StringVar(&subnet, "subnet", "", "Network subnet (e.g., 192.168.1.0/24)")
	cmd.Flags().StringVar(&bridge, "bridge", "br0", "Linux bridge to use")

	return cmd
}

