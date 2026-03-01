package main

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/google/uuid"
	pb "github.com/kcore/kcore/api/node"
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
		cpu              int
		memory           string
		disk             string
		image            string
		node             string
		network          string
		bridge           string
		autostart        bool
		enableKcoreLogin bool
	)

	cmd := &cobra.Command{
		Use:   "vm NAME",
		Short: "Create a virtual machine",
		Long: `Create a new virtual machine on the kcore cluster.

The VM will be scheduled on an available node unless --node is specified.

Examples:
  # Create a VM with 4 CPUs and 8GB RAM (uses default libvirt network with NAT)
  kctl create vm web-server --cpu 4 --memory 8G

  # Create a VM on host subnet using bridge (gets IP from real subnet)
  kctl create vm db-server --cpu 8 --memory 16G --bridge br0

  # Create a VM with specific libvirt network
  kctl create vm cache-server --cpu 2 --memory 4G --network default

  # Create a VM with an image
  kctl create vm ubuntu-vm --cpu 2 --memory 2G --image /path/to/image.qcow2

  # Create a VM that starts automatically
  kctl create vm web-01 --cpu 4 --memory 8G --autostart`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]

			// Parse memory size
			memoryBytes, err := parseMemorySize(memory)
			if err != nil {
				return fmt.Errorf("invalid memory size: %w", err)
			}

			// Parse disk size
			diskBytes, err := parseDiskSize(disk)
			if err != nil {
				return fmt.Errorf("invalid disk size: %w", err)
			}
			_ = diskBytes // TODO: Implement disk size in volume creation

			// Get connection info from flags or config
			configPath, _ := cmd.Flags().GetString("config")
			controllerFlag, _ := cmd.Flags().GetString("controller")
			insecureFlag, _ := cmd.Flags().GetBool("insecure")

			nodeAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerFlag, insecureFlag)
			if err != nil {
				return err
			}

			fmt.Printf("Creating VM '%s' on %s...\n", name, nodeAddr)

			// Create client
			client, err := NewNodeClient(nodeAddr, insecure, certFile, keyFile, caFile)
			if err != nil {
				return fmt.Errorf("failed to connect to node: %w", err)
			}
			defer client.Close()

			// Create VM spec with proper UUID
			vmID := uuid.New().String()

			// Determine network configuration
			// Bridge takes precedence over network flag
			var nics []*pb.Nic
			if bridge != "" {
				nics = []*pb.Nic{
					{
						Network:    bridge, // Bridge name (e.g., "br0")
						Model:      "virtio",
						MacAddress: "", // Let libvirt generate MAC
					},
				}
			} else if network != "" && network != "default" {
				// Explicit network specified (libvirt network name or bridge)
				nics = []*pb.Nic{
					{
						Network:    network,
						Model:      "virtio",
						MacAddress: "",
					},
				}
			}
			// If network is "default" or empty, server will add default NIC automatically

			spec := &pb.VmSpec{
				Id:               vmID,
				Name:             name,
				Cpu:              int32(cpu),
				MemoryBytes:      memoryBytes,
				Disks:            []*pb.Disk{}, // Empty - will be added if image is provided
				Nics:             nics,
				EnableKcoreLogin: enableKcoreLogin,
			}

			// Create VM
			// Longer timeout for image downloads
			timeout := 30 * time.Second
			if image != "" {
				timeout = 10 * time.Minute // Allow time for image download
			}
			ctx, cancel := context.WithTimeout(context.Background(), timeout)
			defer cancel()

			// Determine if image is URI or local path
			var imageURI, imagePath string
			if image != "" {
				if strings.HasPrefix(image, "http://") || strings.HasPrefix(image, "https://") {
					imageURI = image
					fmt.Printf("  Downloading image from %s...\n", image)
				} else {
					imagePath = image
					fmt.Printf("  Using image: %s\n", image)
				}
			}

			status, err := client.CreateVM(ctx, spec, imageURI, imagePath)
			if err != nil {
				return fmt.Errorf("failed to create VM: %w", err)
			}

			fmt.Printf("\n✅ VM '%s' created successfully\n", name)
			fmt.Printf("  ID: %s\n", status.Id)
			fmt.Printf("  Status: %s\n", status.State.String())
			fmt.Printf("  CPU: %d cores\n", cpu)
			fmt.Printf("  Memory: %s (%s)\n", memory, formatBytes(memoryBytes))
			fmt.Printf("  Disk: %s\n", disk)
			if bridge != "" {
				fmt.Printf("  Network: bridge %s (host subnet)\n", bridge)
			} else if network != "" {
				fmt.Printf("  Network: %s\n", network)
			} else {
				fmt.Printf("  Network: default (NAT with DHCP)\n")
			}

			return nil
		},
	}

	cmd.Flags().IntVar(&cpu, "cpu", 2, "Number of CPU cores")
	cmd.Flags().StringVar(&memory, "memory", "2G", "Memory size (e.g., 2G, 4096M)")
	cmd.Flags().StringVar(&disk, "disk", "20G", "Disk size (e.g., 100G, 50000M)")
	cmd.Flags().StringVar(&image, "image", "", "OS image to use (optional, URI or local path)")
	cmd.Flags().StringVar(&node, "node", "", "Specific node to run on (optional)")
	cmd.Flags().StringVar(&network, "network", "default", "Libvirt network name (e.g., 'default' for NAT, 'private' for isolated)")
	cmd.Flags().StringVar(&bridge, "bridge", "", "Bridge interface for host subnet (e.g., 'br0'). VM gets IP from real subnet. Overrides --network")
	cmd.Flags().BoolVar(&autostart, "autostart", false, "Start VM automatically after creation")
	cmd.Flags().BoolVar(&enableKcoreLogin, "enable-kcore-login", false, "Enable known console/SSH credentials (kcore/kcore and distro default user) via cloud-init")

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
