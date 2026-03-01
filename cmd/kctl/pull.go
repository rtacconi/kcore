package main

import (
	"context"
	"fmt"
	"time"

	pb "github.com/kcore/kcore/api/node"
	"github.com/spf13/cobra"
)

func newPullCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "pull RESOURCE",
		Short: "Pull and cache resources (images)",
		Long: `Pull resources from remote locations and cache them locally.

Available resource types:
  image    Pull and cache a VM image`,
	}

	cmd.AddCommand(newPullImageCmd())

	return cmd
}

func newPullImageCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "image URI",
		Short: "Pull and cache a VM image",
		Long: `Pull a VM image from an HTTP/HTTPS URI and cache it on the node.

This pre-downloads images so subsequent VM creations are faster.
Images are cached in /var/lib/kcore/images/ on the node.

Examples:
  # Pull Ubuntu 24.04 cloud image
  kctl pull image https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg-amd64.img

  # Pull Debian 12 cloud image
  kctl pull image https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-nocloud-amd64.qcow2
  
  # Pull to specific node
  kctl pull image https://example.com/image.qcow2 --controller node1:9091`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			imageURI := args[0]

			// Get connection info from flags or config
			configPath, _ := cmd.Flags().GetString("config")
			controllerFlag, _ := cmd.Flags().GetString("controller")
			insecureFlag, _ := cmd.Flags().GetBool("insecure")

			nodeAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerFlag, insecureFlag)
			if err != nil {
				return err
			}

			fmt.Printf("Pulling image from %s...\n", imageURI)
			fmt.Printf("Target node: %s\n\n", nodeAddr)

			// Create client
			client, err := NewNodeClient(nodeAddr, insecure, certFile, keyFile, caFile)
			if err != nil {
				return fmt.Errorf("failed to connect to node: %w", err)
			}
			defer client.Close()

			// Pull image (with generous timeout for downloads)
			ctx, cancel := context.WithTimeout(context.Background(), 15*time.Minute)
			defer cancel()

			resp, err := client.compute.PullImage(ctx, &pb.PullImageRequest{
				Uri: imageURI,
			})
			if err != nil {
				return fmt.Errorf("failed to pull image: %w", err)
			}

			if resp.Cached {
				fmt.Printf("✅ Image already cached\n")
			} else {
				fmt.Printf("✅ Image downloaded successfully\n")
			}
			fmt.Printf("  Path: %s\n", resp.Path)
			fmt.Printf("  Size: %s\n", formatBytes(resp.SizeBytes))
			fmt.Printf("\nYou can now create VMs with:\n")
			fmt.Printf("  kctl create vm my-vm --cpu 2 --memory 4G --image \"%s\"\n", resp.Path)

			return nil
		},
	}

	return cmd
}
