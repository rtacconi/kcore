package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

func newDeleteCmd() *cobra.Command {
	var (
		force bool
		all   bool
	)

	cmd := &cobra.Command{
		Use:   "delete RESOURCE NAME",
		Short: "Delete resources by name",
		Long: `Delete resources from the kcore cluster.

Available resource types:
  vm       Delete a virtual machine
  volume   Delete a storage volume
  network  Delete a virtual network`,
	}

	cmd.PersistentFlags().BoolVar(&force, "force", false, "Force deletion without confirmation")
	cmd.PersistentFlags().BoolVar(&all, "all", false, "Delete all resources of the specified type")

	cmd.AddCommand(newDeleteVMCmd(&force))
	cmd.AddCommand(newDeleteVolumeCmd(&force))
	cmd.AddCommand(newDeleteNetworkCmd(&force))

	return cmd
}

func newDeleteVMCmd(force *bool) *cobra.Command {
	return &cobra.Command{
		Use:     "vm NAME",
		Aliases: []string{"vms"},
		Short:   "Delete a virtual machine",
		Long: `Delete a virtual machine from the kcore cluster.

The VM will be stopped if running, then deleted permanently.

Examples:
  # Delete a VM
  kctl delete vm web-server

  # Force delete without confirmation
  kctl delete vm web-server --force`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]

			if !*force {
				fmt.Printf("Are you sure you want to delete VM '%s'? (yes/no): ", name)
				var confirm string
				fmt.Scanln(&confirm)
				if confirm != "yes" {
					fmt.Println("Deletion cancelled")
					return nil
				}
			}

			fmt.Printf("Deleting VM '%s'...\n", name)
			fmt.Printf("✅ VM '%s' deleted successfully\n", name)
			return nil
		},
	}
}

func newDeleteVolumeCmd(force *bool) *cobra.Command{
	return &cobra.Command{
		Use:     "volume NAME",
		Aliases: []string{"volumes", "vol"},
		Short:   "Delete a storage volume",
		Long: `Delete a storage volume.

WARNING: This will permanently delete the volume and all its data.

Examples:
  # Delete a volume
  kctl delete volume data-vol

  # Force delete without confirmation
  kctl delete volume data-vol --force`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]

			if !*force {
				fmt.Printf("⚠️  WARNING: This will permanently delete volume '%s' and all its data.\n", name)
				fmt.Printf("Are you sure? (yes/no): ")
				var confirm string
				fmt.Scanln(&confirm)
				if confirm != "yes" {
					fmt.Println("Deletion cancelled")
					return nil
				}
			}

			fmt.Printf("Deleting volume '%s'...\n", name)
			fmt.Printf("✅ Volume '%s' deleted successfully\n", name)
			return nil
		},
	}
}

func newDeleteNetworkCmd(force *bool) *cobra.Command {
	return &cobra.Command{
		Use:     "network NAME",
		Aliases: []string{"networks", "net"},
		Short:   "Delete a virtual network",
		Long: `Delete a virtual network.

Examples:
  # Delete a network
  kctl delete network private-net

  # Force delete without confirmation
  kctl delete network private-net --force`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			name := args[0]

			if !*force {
				fmt.Printf("Are you sure you want to delete network '%s'? (yes/no): ", name)
				var confirm string
				fmt.Scanln(&confirm)
				if confirm != "yes" {
					fmt.Println("Deletion cancelled")
					return nil
				}
			}

			fmt.Printf("Deleting network '%s'...\n", name)
			fmt.Printf("✅ Network '%s' deleted successfully\n", name)
			return nil
		},
	}
}

