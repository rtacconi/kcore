package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

var (
	version = "0.1.0"
)

func main() {
	rootCmd := &cobra.Command{
		Use:   "kctl",
		Short: "kctl - kcore cluster management CLI",
		Long: `kctl is the command-line interface for kcore.

It provides commands to create, manage, and delete VMs across your
kcore cluster. Think of it as kubectl for your virtualization infrastructure.

Examples:
  # Create a VM
  kctl create vm my-vm --cpu 4 --memory 8G --disk 100G

  # List all VMs
  kctl get vms

  # Get VM details
  kctl get vm my-vm

  # Delete a VM
  kctl delete vm my-vm

  # List all nodes
  kctl get nodes`,
		Version: version,
	}

	// Global flags
	rootCmd.PersistentFlags().StringP("config", "c", "~/.kcore/config", "Path to kctl config file")
	rootCmd.PersistentFlags().StringP("controller", "s", "", "Controller address (overrides config)")
	rootCmd.PersistentFlags().BoolP("insecure", "k", false, "Skip TLS certificate verification")

	// Add subcommands
	rootCmd.AddCommand(newCreateCmd())
	rootCmd.AddCommand(newGetCmd())
	rootCmd.AddCommand(newDeleteCmd())
	rootCmd.AddCommand(newDescribeCmd())
	rootCmd.AddCommand(newPullCmd())
	rootCmd.AddCommand(newApplyCmd())
	rootCmd.AddCommand(newVersionCmd())

	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}

func newVersionCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Print the kctl version",
		Run: func(cmd *cobra.Command, args []string) {
			fmt.Printf("kctl version %s\n", version)
		},
	}
}
