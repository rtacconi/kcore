package main

import (
	"context"
	"fmt"
	"time"

	ctrlpb "github.com/kcore/kcore/api/controller"
	"github.com/spf13/cobra"
)

func addGetDisksCmd(getCmd *cobra.Command) {
	var nodeID string

	cmd := &cobra.Command{
		Use:   "disks",
		Short: "List disks on a node",
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

			ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
			defer cancel()

			resp, err := client.api.ListNodeDisks(ctx, &ctrlpb.ListNodeDisksRequest{NodeId: nodeID})
			if err != nil {
				return fmt.Errorf("list disks: %w", err)
			}

			if resp.BootstrapMode {
				fmt.Println("(bootstrap mode)")
			}

			fmt.Printf("%-12s %-15s %-20s %-15s %-5s %-10s\n", "NAME", "SIZE", "MODEL", "SERIAL", "RM", "PARTS")
			for _, d := range resp.Disks {
				fmt.Printf("%-12s %-15s %-20s %-15s %-5t %-10t\n",
					d.Name,
					formatBytes(int64(d.SizeBytes)),
					truncate(d.Model, 20),
					truncate(d.Serial, 15),
					d.Removable,
					d.HasPartitions,
				)
			}
			return nil
		},
	}

	cmd.Flags().StringVar(&nodeID, "node", "", "Node ID (required)")
	cmd.MarkFlagRequired("node")
	getCmd.AddCommand(cmd)
}

func truncate(s string, maxLen int) string {
	if len(s) > maxLen {
		return s[:maxLen-3] + "..."
	}
	return s
}
