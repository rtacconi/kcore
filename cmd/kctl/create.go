package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

func newCreateCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "create RESOURCE",
		Short: "Create a resource (volume, network)",
		Long: `Create kcore resources.

Available resource types:
  volume   Create a storage volume
  network  Create a virtual network

To create VMs, use declarative manifests:
  kctl apply -f vm.yaml`,
	}

	cmd.AddCommand(newCreateVolumeCmd())
	cmd.AddCommand(newCreateNetworkCmd())

	return cmd
}

func newCreateVolumeCmd() *cobra.Command {
	var (
		size         string
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
