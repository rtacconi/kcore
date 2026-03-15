package main

import (
	"context"
	"fmt"
	"strings"
	"time"

	ctrlpb "github.com/kcore/kcore/api/controller"
	"github.com/spf13/cobra"
)

func newConfigureCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "configure",
		Short: "Configure node resources",
	}

	cmd.AddCommand(newConfigureNetworkCmd())
	return cmd
}

func newConfigureNetworkCmd() *cobra.Command {
	var (
		nodeID     string
		bridgeName string
		bridgePorts string
		bridgeDHCP bool
		bridgeIP   string
		dnsServers string
		applyNow   bool
	)

	cmd := &cobra.Command{
		Use:   "network",
		Short: "Configure network interfaces on a node",
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

			req := &ctrlpb.ConfigureNetworkRequest{
				NodeId:     nodeID,
				DnsServers: dnsServers,
				ApplyNow:   applyNow,
			}

			if bridgeName != "" {
				var ports []string
				if bridgePorts != "" {
					ports = strings.Split(bridgePorts, ",")
				}
				req.Bridges = append(req.Bridges, &ctrlpb.BridgeConfig{
					Name:        bridgeName,
					MemberPorts: ports,
					Dhcp:        bridgeDHCP,
					IpAddress:   bridgeIP,
				})
			}

			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
			defer cancel()

			resp, err := client.api.ConfigureNetwork(ctx, req)
			if err != nil {
				return fmt.Errorf("configure network: %w", err)
			}

			if resp.Success {
				fmt.Println("Network configured successfully")
			} else {
				fmt.Printf("Network configuration failed: %s\n", resp.Message)
			}

			if resp.GeneratedNixSnippet != "" {
				fmt.Printf("\nGenerated NixOS config:\n%s\n", resp.GeneratedNixSnippet)
			}
			return nil
		},
	}

	cmd.Flags().StringVar(&nodeID, "node", "", "Node ID (required)")
	cmd.Flags().StringVar(&bridgeName, "bridge", "", "Bridge name (e.g., br0)")
	cmd.Flags().StringVar(&bridgePorts, "bridge-ports", "", "Bridge member ports (comma-separated)")
	cmd.Flags().BoolVar(&bridgeDHCP, "bridge-dhcp", false, "Enable DHCP on bridge")
	cmd.Flags().StringVar(&bridgeIP, "bridge-ip", "", "Static IP for bridge")
	cmd.Flags().StringVar(&dnsServers, "dns", "", "DNS servers (comma-separated)")
	cmd.Flags().BoolVar(&applyNow, "apply", false, "Apply and rebuild now")
	cmd.MarkFlagRequired("node")

	return cmd
}
