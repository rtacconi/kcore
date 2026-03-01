package main

import (
	"context"
	"fmt"
	"os"
	"time"

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

			// Read file
			data, err := os.ReadFile(filename)
			if err != nil {
				return fmt.Errorf("failed to read file: %w", err)
			}

			if dryRun {
				fmt.Printf("Applying configuration from %s (dry run):\n\n", filename)
				fmt.Printf("%s\n\n", string(data))
				fmt.Printf("✅ Dry run complete - no changes made\n")
				return nil
			}

			fmt.Printf("Applying configuration from %s to controller...\n", filename)

			// Use global flags provided on root command
			root := cmd.Root()
			configPath, _ := root.PersistentFlags().GetString("config")
			controllerAddr, _ := root.PersistentFlags().GetString("controller")
			insecureFlag, _ := root.PersistentFlags().GetBool("insecure")

			ctrlAddr, insecureTLS, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerAddr, insecureFlag)
			if err != nil {
				return err
			}

			client, err := NewControllerAdminClient(ctrlAddr, insecureTLS, certFile, keyFile, caFile)
			if err != nil {
				return fmt.Errorf("failed to create controller admin client: %w", err)
			}
			defer client.Close()

			ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
			defer cancel()

			// For now, always rebuild after applying
			resp, err := client.ApplyNixConfig(ctx, string(data), true)
			if err != nil {
				return fmt.Errorf("ApplyNixConfig RPC failed: %w", err)
			}
			if !resp.GetSuccess() {
				return fmt.Errorf("controller reported failure: %s", resp.GetMessage())
			}

			fmt.Printf("✅ Controller configuration applied: %s\n", resp.GetMessage())
			return nil
		},
	}

	cmd.Flags().StringVarP(&filename, "filename", "f", "", "Filename to apply (required)")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "Show what would be applied without applying")
	cmd.MarkFlagRequired("filename")

	return cmd
}
