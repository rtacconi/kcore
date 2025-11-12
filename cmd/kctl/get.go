package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

func newGetCmd() *cobra.Command {
	var (
		allNamespaces bool
		output        string
		selector      string
	)

	cmd := &cobra.Command{
		Use:   "get RESOURCE [NAME]",
		Short: "Display one or many resources",
		Long: `Display one or many resources from the kcore cluster.

Available resource types:
  vms, vm          List or get virtual machines
  nodes, node      List or get nodes
  volumes, volume  List or get storage volumes
  networks         List or get networks`,
	}

	cmd.PersistentFlags().BoolVarP(&allNamespaces, "all-namespaces", "A", false, "List resources across all namespaces")
	cmd.PersistentFlags().StringVarP(&output, "output", "o", "", "Output format (json, yaml, wide)")
	cmd.PersistentFlags().StringVarP(&selector, "selector", "l", "", "Selector (label query) to filter on")

	cmd.AddCommand(newGetVMsCmd(&output))
	cmd.AddCommand(newGetNodesCmd(&output))
	cmd.AddCommand(newGetVolumesCmd(&output))
	cmd.AddCommand(newGetNetworksCmd(&output))

	return cmd
}

func newGetVMsCmd(output *string) *cobra.Command {
	return &cobra.Command{
		Use:     "vms [NAME]",
		Aliases: []string{"vm"},
		Short:   "List virtual machines",
		Long: `List all virtual machines or get details of a specific VM.

Examples:
  # List all VMs
  kctl get vms

  # Get specific VM
  kctl get vm web-server

  # List VMs with wide output
  kctl get vms -o wide

  # Get VM in JSON format
  kctl get vm web-server -o json`,
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) == 0 {
				// List all VMs
				return listVMs(*output)
			}
			// Get specific VM
			return getVM(args[0], *output)
		},
	}
}

func newGetNodesCmd(output *string) *cobra.Command {
	return &cobra.Command{
		Use:     "nodes [NAME]",
		Aliases: []string{"node"},
		Short:   "List cluster nodes",
		Long: `List all nodes in the kcore cluster or get details of a specific node.

Examples:
  # List all nodes
  kctl get nodes

  # Get specific node
  kctl get node kvm-node-01

  # List nodes with wide output
  kctl get nodes -o wide`,
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) == 0 {
				return listNodes(*output)
			}
			return getNode(args[0], *output)
		},
	}
}

func newGetVolumesCmd(output *string) *cobra.Command {
	return &cobra.Command{
		Use:     "volumes [NAME]",
		Aliases: []string{"volume", "vol"},
		Short:   "List storage volumes",
		Long: `List all storage volumes or get details of a specific volume.

Examples:
  # List all volumes
  kctl get volumes

  # Get specific volume
  kctl get volume data-vol`,
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) == 0 {
				return listVolumes(*output)
			}
			return getVolume(args[0], *output)
		},
	}
}

func newGetNetworksCmd(output *string) *cobra.Command {
	return &cobra.Command{
		Use:     "networks [NAME]",
		Aliases: []string{"network", "net"},
		Short:   "List virtual networks",
		Long: `List all virtual networks or get details of a specific network.

Examples:
  # List all networks
  kctl get networks

  # Get specific network
  kctl get network private-net`,
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) == 0 {
				return listNetworks(*output)
			}
			return getNetwork(args[0], *output)
		},
	}
}

// Implementation functions (TODO: connect to controller gRPC API)

func listVMs(output string) error {
	fmt.Println("NAME          STATUS    NODE           CPU    MEMORY    AGE")
	fmt.Println("web-server    Running   kvm-node-01    4      8G        2d")
	fmt.Println("db-server     Running   kvm-node-02    8      16G       5d")
	fmt.Println("cache-01      Stopped   kvm-node-01    2      4G        1d")
	return nil
}

func getVM(name, output string) error {
	fmt.Printf("Name:         %s\n", name)
	fmt.Printf("Status:       Running\n")
	fmt.Printf("Node:         kvm-node-01\n")
	fmt.Printf("CPU:          4 cores\n")
	fmt.Printf("Memory:       8GB\n")
	fmt.Printf("Disk:         100GB\n")
	fmt.Printf("Network:      default\n")
	fmt.Printf("IP Address:   192.168.1.100\n")
	fmt.Printf("Autostart:    false\n")
	fmt.Printf("Age:          2d\n")
	return nil
}

func listNodes(output string) error {
	fmt.Println("NAME           STATUS    ROLE      VERSION    VMS    CPU       MEMORY")
	fmt.Println("kvm-node-01    Ready     compute   0.1.0      5      32/64     64/128GB")
	fmt.Println("kvm-node-02    Ready     compute   0.1.0      3      16/64     32/128GB")
	fmt.Println("kvm-node-03    Ready     compute   0.1.0      8      48/64     96/128GB")
	return nil
}

func getNode(name, output string) error {
	fmt.Printf("Name:              %s\n", name)
	fmt.Printf("Status:            Ready\n")
	fmt.Printf("Role:              compute\n")
	fmt.Printf("Version:           0.1.0\n")
	fmt.Printf("IP Address:        192.168.1.50\n")
	fmt.Printf("VMs Running:       5\n")
	fmt.Printf("CPU:               32 used / 64 total\n")
	fmt.Printf("Memory:            64GB used / 128GB total\n")
	fmt.Printf("Storage:           2TB used / 4TB total\n")
	fmt.Printf("Last Heartbeat:    30s ago\n")
	return nil
}

func listVolumes(output string) error {
	fmt.Println("NAME           SIZE     STORAGE-CLASS    NODE           STATUS")
	fmt.Println("data-vol       100G     local-lvm        kvm-node-01    Available")
	fmt.Println("db-vol         500G     local-lvm        kvm-node-02    Bound")
	return nil
}

func getVolume(name, output string) error {
	fmt.Printf("Name:             %s\n", name)
	fmt.Printf("Size:             100GB\n")
	fmt.Printf("Storage Class:    local-lvm\n")
	fmt.Printf("Node:             kvm-node-01\n")
	fmt.Printf("Status:           Available\n")
	return nil
}

func listNetworks(output string) error {
	fmt.Println("NAME           SUBNET              BRIDGE    VMS")
	fmt.Println("default        192.168.1.0/24      br0       15")
	fmt.Println("private-net    192.168.100.0/24    br1       3")
	return nil
}

func getNetwork(name, output string) error {
	fmt.Printf("Name:        %s\n", name)
	fmt.Printf("Subnet:      192.168.1.0/24\n")
	fmt.Printf("Bridge:      br0\n")
	fmt.Printf("VMs:         15\n")
	return nil
}

