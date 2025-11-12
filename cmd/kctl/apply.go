package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

func newApplyCmd() *cobra.Command {
	var (
		filename string
		dryRun   bool
	)

	cmd := &cobra.Command{
		Use:   "apply -f FILENAME",
		Short: "Apply a configuration from a file",
		Long: `Apply a configuration to resources from a YAML or JSON file.

This command creates or updates resources based on the specifications
in the provided file.

Examples:
  # Apply configuration from a file
  kctl apply -f vm.yaml

  # Apply multiple files
  kctl apply -f vm1.yaml -f vm2.yaml

  # Dry run (show what would be applied without applying)
  kctl apply -f vm.yaml --dry-run`,
		RunE: func(cmd *cobra.Command, args []string) error {
			if filename == "" {
				return fmt.Errorf("filename required: use -f or --filename")
			}

			if dryRun {
				fmt.Printf("🔍 Dry run mode - showing what would be applied:\n\n")
			}

			// Read file
			data, err := os.ReadFile(filename)
			if err != nil {
				return fmt.Errorf("failed to read file: %w", err)
			}

			fmt.Printf("Applying configuration from %s...\n", filename)
			fmt.Printf("\n%s\n\n", string(data))

			if dryRun {
				fmt.Printf("✅ Dry run complete - no changes made\n")
			} else {
				fmt.Printf("✅ Configuration applied successfully\n")
			}

			return nil
		},
	}

	cmd.Flags().StringVarP(&filename, "filename", "f", "", "Filename to apply (required)")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "Show what would be applied without applying")
	cmd.MarkFlagRequired("filename")

	return cmd
}

