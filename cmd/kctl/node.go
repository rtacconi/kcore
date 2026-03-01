package main

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/spf13/cobra"
)

// newNodeCmd groups node-related helper commands under `kctl node`.
func newNodeCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "node",
		Short: "Node-level operations (bootstrap, config)",
		Long: `Node-level operations for kcore nodes.

These commands talk directly to the node-agent's admin API (NodeAdmin)
and are intended for cluster automation and bootstrapping.`,
	}

	cmd.AddCommand(newNodeApplyNixCmd())
	return cmd
}

// newNodeApplyNixCmd applies a NixOS configuration to a node via NodeAdmin.ApplyNixConfig.
// This is the node-side equivalent of "kctl apply" (which targets the controller).
func newNodeApplyNixCmd() *cobra.Command {
	var (
		filename    string
		nodeAddr    string
		noRebuild   bool
		rebuildFlag bool
	)

	c := &cobra.Command{
		Use:   "apply-nix",
		Short: "Apply a NixOS configuration to a node (via NodeAdmin.ApplyNixConfig)",
		Long: `Apply a full /etc/nixos/configuration.nix to a node via the node-agent's admin API.

Examples:
  # Apply configuration.nix to a node at 192.168.40.146:9091 and rebuild
  kctl node apply-nix --node 192.168.40.146:9091 -f configuration.nix

  # Apply configuration without triggering nixos-rebuild (write-only)
  kctl node apply-nix --node 192.168.40.146:9091 -f configuration.nix --no-rebuild

If --node is omitted, the address from the current kctl context/flags will be used.`,
		RunE: func(cmd *cobra.Command, args []string) error {
			if filename == "" {
				return fmt.Errorf("filename required: use -f or --filename")
			}

			data, err := os.ReadFile(filename)
			if err != nil {
				return fmt.Errorf("failed to read file: %w", err)
			}

			root := cmd.Root()
			configPath, _ := root.PersistentFlags().GetString("config")
			controllerFlag, _ := root.PersistentFlags().GetString("controller")
			insecureFlag, _ := root.PersistentFlags().GetBool("insecure")

			// Determine node address:
			// - if --node was given, use it directly
			// - otherwise, fall back to the same resolution as other commands
			addr := nodeAddr
			var insecureTLS bool
			var certFile, keyFile, caFile string

			if addr == "" {
				// Reuse existing connection-info logic; address may point directly
				// at a node-agent in smaller setups.
				a, insecure, cert, key, ca, err := GetConnectionInfo(configPath, controllerFlag, insecureFlag)
				if err != nil {
					return err
				}
				addr = a
				insecureTLS = insecure
				certFile = cert
				keyFile = key
				caFile = ca
			} else {
				// Normalize address and reuse the same TLS defaults as controller.
				normalized := NormalizeAddress(addr)
				addr = normalized
				// For now, use the default controller cert paths when explicit node
				// address is provided. This keeps cert handling in one place.
				insecureTLS = insecureFlag
				certFile = "certs/controller.crt"
				keyFile = "certs/controller.key"
				caFile = "certs/ca.crt"
			}

			if !noRebuild {
				rebuildFlag = true
			}

			fmt.Printf("Applying NixOS configuration from %s to node %s (rebuild=%v)...\n", filename, addr, rebuildFlag)

			client, err := NewNodeAdminClient(addr, insecureTLS, certFile, keyFile, caFile)
			if err != nil {
				return fmt.Errorf("failed to create node admin client: %w", err)
			}
			defer client.Close()

			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
			defer cancel()

			resp, err := client.ApplyNixConfig(ctx, string(data), rebuildFlag)
			if err != nil {
				return fmt.Errorf("ApplyNixConfig RPC failed: %w", err)
			}

			if !resp.GetSuccess() {
				return fmt.Errorf("node reported failure: %s", resp.GetMessage())
			}

			fmt.Printf("✅ Node configuration applied: %s\n", resp.GetMessage())
			return nil
		},
	}

	c.Flags().StringVarP(&filename, "filename", "f", "", "NixOS configuration file to apply (required)")
	c.Flags().StringVar(&nodeAddr, "node", "", "Node address (host or host:port). Defaults to controller address from kctl config")
	c.Flags().BoolVar(&noRebuild, "no-rebuild", false, "Do not run nixos-rebuild after writing configuration.nix")
	c.MarkFlagRequired("filename")

	return c
}


