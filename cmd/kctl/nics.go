package main

import (
	"context"
	"fmt"
	"time"

	ctrlpb "github.com/kcore/kcore/api/controller"
	"github.com/spf13/cobra"
)

func addGetNicsCmd(getCmd *cobra.Command) {
	var nodeID string

	cmd := &cobra.Command{
		Use:   "nics",
		Short: "List network interfaces on a node",
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

			resp, err := client.api.ListNetworkInterfaces(ctx, &ctrlpb.ListNetworkInterfacesRequest{NodeId: nodeID})
			if err != nil {
				return fmt.Errorf("list nics: %w", err)
			}

			fmt.Printf("%-16s %-18s %-16s %-5s %-8s %-12s %-7s\n",
				"NAME", "MAC", "IP", "UP", "SPEED", "DRIVER", "VIRTUAL")
			for _, n := range resp.Interfaces {
				fmt.Printf("%-16s %-18s %-16s %-5t %-8s %-12s %-7t\n",
					n.Name,
					n.MacAddress,
					n.IpAddress,
					n.IsUp,
					formatSpeed(n.SpeedMbps),
					n.Driver,
					n.IsVirtual,
				)
			}
			return nil
		},
	}

	cmd.Flags().StringVar(&nodeID, "node", "", "Node ID (required)")
	cmd.MarkFlagRequired("node")
	getCmd.AddCommand(cmd)
}

func formatSpeed(mbps int64) string {
	if mbps <= 0 {
		return "-"
	}
	if mbps >= 1000 {
		return fmt.Sprintf("%dG", mbps/1000)
	}
	return fmt.Sprintf("%dM", mbps)
}
