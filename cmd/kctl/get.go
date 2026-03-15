package main

import (
	"context"
	"fmt"
	"time"

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

	addGetDisksCmd(cmd)
	addGetNicsCmd(cmd)

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
			configPath, _ := cmd.Flags().GetString("config")
			controllerFlag, _ := cmd.Flags().GetString("controller")
			insecureFlag, _ := cmd.Flags().GetBool("insecure")

			if len(args) == 0 {
				return listVMs(*output, configPath, controllerFlag, insecureFlag)
			}
			return getVM(args[0], *output, configPath, controllerFlag, insecureFlag)
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
  kctl get node kvm-node-01`,
		RunE: func(cmd *cobra.Command, args []string) error {
			configPath, _ := cmd.Flags().GetString("config")
			controllerFlag, _ := cmd.Flags().GetString("controller")
			insecureFlag, _ := cmd.Flags().GetBool("insecure")

			if len(args) == 0 {
				return listNodes(*output, configPath, controllerFlag, insecureFlag)
			}
			return getNode(args[0], *output, configPath, controllerFlag, insecureFlag)
		},
	}
}

func newGetVolumesCmd(output *string) *cobra.Command {
	return &cobra.Command{
		Use:     "volumes [NAME]",
		Aliases: []string{"volume", "vol"},
		Short:   "List storage volumes",
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
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) == 0 {
				return listNetworks(*output)
			}
			return getNetwork(args[0], *output)
		},
	}
}

// --- VM operations (through controller) ------------------------------------

func listVMs(output, configPath, controllerFlag string, insecureFlag bool) error {
	ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerFlag, insecureFlag)
	if err != nil {
		return err
	}

	client, err := NewControllerVMClient(ctrlAddr, insecure, certFile, keyFile, caFile)
	if err != nil {
		return fmt.Errorf("failed to connect to controller: %w", err)
	}
	defer client.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	vms, err := client.ListVMs(ctx)
	if err != nil {
		return fmt.Errorf("failed to list VMs: %w", err)
	}

	fmt.Printf("%-36s  %-20s %-12s %-6s %-10s %-20s\n", "ID", "NAME", "STATUS", "CPU", "MEMORY", "NODE")
	for _, vm := range vms {
		stateStr := vm.State.String()[9:] // strip "VM_STATE_" prefix
		memory := formatBytes(vm.MemoryBytes)
		fmt.Printf("%-36s  %-20s %-12s %-6d %-10s %-20s\n",
			vm.Id, vm.Name, stateStr, vm.Cpu, memory, vm.NodeId)
	}

	return nil
}

func getVM(name, output, configPath, controllerFlag string, insecureFlag bool) error {
	ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerFlag, insecureFlag)
	if err != nil {
		return err
	}

	client, err := NewControllerVMClient(ctrlAddr, insecure, certFile, keyFile, caFile)
	if err != nil {
		return fmt.Errorf("failed to connect to controller: %w", err)
	}
	defer client.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	resp, err := client.GetVM(ctx, name)
	if err != nil {
		return fmt.Errorf("failed to get VM: %w", err)
	}

	fmt.Printf("Name:         %s\n", name)
	if resp.Status != nil {
		fmt.Printf("ID:           %s\n", resp.Status.Id)
		fmt.Printf("Status:       %s\n", resp.Status.State.String()[9:])
	}
	fmt.Printf("Node:         %s\n", resp.NodeId)

	if resp.Spec != nil {
		fmt.Printf("CPU:          %d cores\n", resp.Spec.Cpu)
		fmt.Printf("Memory:       %s\n", formatBytes(resp.Spec.MemoryBytes))

		if len(resp.Spec.Disks) > 0 {
			fmt.Printf("Disks:\n")
			for _, disk := range resp.Spec.Disks {
				fmt.Printf("  - %s: %s\n", disk.Device, disk.BackendHandle)
			}
		}

		if len(resp.Spec.Nics) > 0 {
			fmt.Printf("Network:\n")
			for _, nic := range resp.Spec.Nics {
				fmt.Printf("  - %s (%s)\n", nic.Network, nic.MacAddress)
			}
		}
	}

	if resp.Status != nil && resp.Status.CreatedAt != nil {
		fmt.Printf("Created:      %s\n", resp.Status.CreatedAt.AsTime().Format(time.RFC3339))
	}

	return nil
}

// --- Node operations (through controller) ----------------------------------

func listNodes(output, configPath, controllerFlag string, insecureFlag bool) error {
	ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerFlag, insecureFlag)
	if err != nil {
		return err
	}

	client, err := NewControllerVMClient(ctrlAddr, insecure, certFile, keyFile, caFile)
	if err != nil {
		return fmt.Errorf("failed to connect to controller: %w", err)
	}
	defer client.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	nodes, err := client.ListNodes(ctx)
	if err != nil {
		return fmt.Errorf("failed to list nodes: %w", err)
	}

	fmt.Printf("%-20s %-20s %-10s %-10s %-12s\n", "ID", "HOSTNAME", "STATUS", "CPU", "MEMORY")
	for _, n := range nodes {
		cpuStr := fmt.Sprintf("%d cores", n.Capacity.GetCpuCores())
		memStr := formatBytes(n.Capacity.GetMemoryBytes())
		fmt.Printf("%-20s %-20s %-10s %-10s %-12s\n",
			n.NodeId, n.Hostname, n.Status, cpuStr, memStr)
	}

	return nil
}

func getNode(name, output, configPath, controllerFlag string, insecureFlag bool) error {
	ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerFlag, insecureFlag)
	if err != nil {
		return err
	}

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

	fmt.Printf("Node ID:       %s\n", node.NodeId)
	fmt.Printf("Hostname:      %s\n", node.Hostname)
	fmt.Printf("Address:       %s\n", node.Address)
	fmt.Printf("Status:        %s\n", node.Status)
	if node.Capacity != nil {
		fmt.Printf("CPU:           %d cores\n", node.Capacity.CpuCores)
		fmt.Printf("Memory:        %s\n", formatBytes(node.Capacity.MemoryBytes))
	}
	if node.Usage != nil {
		fmt.Printf("CPU Used:      %d cores\n", node.Usage.CpuCoresUsed)
		fmt.Printf("Memory Used:   %s\n", formatBytes(node.Usage.MemoryBytesUsed))
	}
	if node.LastHeartbeat != nil {
		fmt.Printf("Last Heartbeat: %s\n", node.LastHeartbeat.AsTime().Format(time.RFC3339))
	}

	return nil
}

// --- Placeholder operations (volumes, networks) ----------------------------

func listVolumes(output string) error {
	fmt.Println("NAME           SIZE     STORAGE-CLASS    NODE           STATUS")
	fmt.Println("(no volumes - future feature)")
	return nil
}

func getVolume(name, output string) error {
	fmt.Printf("Volume '%s' not found (volumes are a future feature)\n", name)
	return nil
}

func listNetworks(output string) error {
	fmt.Println("NAME           SUBNET              BRIDGE    VMS")
	fmt.Println("(no networks - future feature)")
	return nil
}

func getNetwork(name, output string) error {
	fmt.Printf("Network '%s' not found (networks are a future feature)\n", name)
	return nil
}
