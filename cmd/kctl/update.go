package main

import (
	"context"
	"fmt"
	"os"
	"time"

	ctrlpb "github.com/kcore/kcore/api/controller"
	"github.com/spf13/cobra"
)

func newUpdateCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "update",
		Short: "Update node configuration and system",
	}

	cmd.AddCommand(newUpdateNixConfigCmd())
	cmd.AddCommand(newUpdateSystemCmd())
	return cmd
}

func newUpdateNixConfigCmd() *cobra.Command {
	var (
		nodeID   string
		filename string
	)

	cmd := &cobra.Command{
		Use:   "nixconfig",
		Short: "Push NixOS configuration to a node",
		RunE: func(cmd *cobra.Command, args []string) error {
			if nodeID == "" {
				return fmt.Errorf("--node is required")
			}
			if filename == "" {
				return fmt.Errorf("--file is required")
			}

			data, err := os.ReadFile(filename)
			if err != nil {
				return fmt.Errorf("read file: %w", err)
			}

			root := cmd.Root()
			configPath, _ := root.PersistentFlags().GetString("config")
			controllerAddr, _ := root.PersistentFlags().GetString("controller")
			insecureFlag, _ := root.PersistentFlags().GetBool("insecure")

			ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerAddr, insecureFlag)
			if err != nil {
				return err
			}

			client, err := NewControllerVMClient(ctrlAddr, insecure, certFile, keyFile, caFile)
			if err != nil {
				return fmt.Errorf("connect to controller: %w", err)
			}
			defer client.Close()

			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
			defer cancel()

			resp, err := client.api.UpdateNixConfig(ctx, &ctrlpb.UpdateNixConfigRequest{
				NodeId:           nodeID,
				ConfigurationNix: string(data),
			})
			if err != nil {
				return fmt.Errorf("update nix config: %w", err)
			}

			if resp.Success {
				fmt.Println("NixOS config pushed successfully")
			} else {
				fmt.Printf("Failed: %s\n", resp.Message)
			}
			return nil
		},
	}

	cmd.Flags().StringVar(&nodeID, "node", "", "Node ID (required)")
	cmd.Flags().StringVarP(&filename, "file", "f", "", "NixOS configuration file (required)")
	cmd.MarkFlagRequired("node")
	cmd.MarkFlagRequired("file")

	return cmd
}

func newUpdateSystemCmd() *cobra.Command {
	var (
		nodeID         string
		updateChannels bool
		rebuild        bool
		updateAgent    bool
	)

	cmd := &cobra.Command{
		Use:   "system",
		Short: "Update node system (channels, rebuild, agent)",
		RunE: func(cmd *cobra.Command, args []string) error {
			if nodeID == "" {
				return fmt.Errorf("--node is required")
			}

			root := cmd.Root()
			configPath, _ := root.PersistentFlags().GetString("config")
			controllerAddr, _ := root.PersistentFlags().GetString("controller")
			insecureFlag, _ := root.PersistentFlags().GetBool("insecure")

			ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerAddr, insecureFlag)
			if err != nil {
				return err
			}

			client, err := NewControllerVMClient(ctrlAddr, insecure, certFile, keyFile, caFile)
			if err != nil {
				return fmt.Errorf("connect to controller: %w", err)
			}
			defer client.Close()

			ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
			defer cancel()

			resp, err := client.api.UpdateSystem(ctx, &ctrlpb.UpdateSystemRequest{
				NodeId:         nodeID,
				UpdateChannels: updateChannels,
				Rebuild:        rebuild,
				UpdateAgent:    updateAgent,
			})
			if err != nil {
				return fmt.Errorf("update system: %w", err)
			}

			if resp.Success {
				fmt.Println("System update completed successfully")
			} else {
				fmt.Printf("Failed: %s\n", resp.Message)
			}
			return nil
		},
	}

	cmd.Flags().StringVar(&nodeID, "node", "", "Node ID (required)")
	cmd.Flags().BoolVar(&updateChannels, "channels", false, "Update NixOS channels")
	cmd.Flags().BoolVar(&rebuild, "rebuild", true, "Run nixos-rebuild switch")
	cmd.Flags().BoolVar(&updateAgent, "agent", false, "Update node-agent binary")
	cmd.MarkFlagRequired("node")

	return cmd
}
