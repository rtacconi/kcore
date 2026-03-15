package main

import (
	"context"
	"fmt"
	"time"

	ctrlpb "github.com/kcore/kcore/api/controller"
	"github.com/spf13/cobra"
)

func newInstallCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "install",
		Short: "Install OS on a node",
	}

	cmd.AddCommand(newInstallNodeCmd())
	return cmd
}

func newInstallNodeCmd() *cobra.Command {
	var (
		nodeID            string
		hostname          string
		rootPassword      string
		osDisk            string
		storageDisks      []string
		sshKeys           []string
		runController     bool
		controllerAddress string
	)

	cmd := &cobra.Command{
		Use:   "node",
		Short: "Install NixOS on a node via the controller",
		RunE: func(cmd *cobra.Command, args []string) error {
			if nodeID == "" {
				return fmt.Errorf("--node is required")
			}
			if osDisk == "" {
				return fmt.Errorf("--os-disk is required")
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

			disks := []*ctrlpb.DiskConfig{{Device: osDisk, Role: "os"}}
			for _, sd := range storageDisks {
				disks = append(disks, &ctrlpb.DiskConfig{Device: sd, Role: "storage"})
			}

			ctx, cancel := context.WithTimeout(context.Background(), 30*time.Minute)
			defer cancel()

			resp, err := client.api.InstallNode(ctx, &ctrlpb.InstallNodeRequest{
				NodeId:            nodeID,
				Disks:             disks,
				Hostname:          hostname,
				RootPassword:      rootPassword,
				SshKeys:           sshKeys,
				RunController:     runController,
				ControllerAddress: controllerAddress,
			})
			if err != nil {
				return fmt.Errorf("install node: %w", err)
			}

			fmt.Printf("Install started: id=%s status=%s\n", resp.InstallId, resp.Status)
			if resp.Message != "" {
				fmt.Printf("  %s\n", resp.Message)
			}
			return nil
		},
	}

	cmd.Flags().StringVar(&nodeID, "node", "", "Node ID to install (required)")
	cmd.Flags().StringVar(&hostname, "hostname", "", "Hostname for the installed node")
	cmd.Flags().StringVar(&rootPassword, "root-password", "", "Root password")
	cmd.Flags().StringVar(&osDisk, "os-disk", "", "Disk for OS installation (required)")
	cmd.Flags().StringSliceVar(&storageDisks, "storage-disk", nil, "Disks for storage (LVM)")
	cmd.Flags().StringSliceVar(&sshKeys, "ssh-key", nil, "SSH public keys to install")
	cmd.Flags().BoolVar(&runController, "run-controller", false, "Run controller on this node after reboot")
	cmd.Flags().StringVar(&controllerAddress, "controller-address", "", "Remote controller to register with after reboot")
	cmd.MarkFlagRequired("node")
	cmd.MarkFlagRequired("os-disk")

	return cmd
}
