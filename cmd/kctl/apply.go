package main

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/google/uuid"
	ctrlpb "github.com/kcore/kcore/api/controller"
	"github.com/spf13/cobra"
)

func newApplyCmd() *cobra.Command {
	var (
		filename string
		dryRun   bool
	)

	cmd := &cobra.Command{
		Use:   "apply -f FILENAME",
		Short: "Apply a configuration from a YAML file",
		Long: `Apply a resource definition from a YAML file.

Supported resource kinds:
  VM / VirtualMachine   Create a virtual machine

Examples:
  # Create a VM from a manifest
  kctl apply -f vm.yaml

  # Dry run (show what would be applied)
  kctl apply -f vm.yaml --dry-run`,
		RunE: func(cmd *cobra.Command, args []string) error {
			if filename == "" {
				return fmt.Errorf("filename required: use -f or --filename")
			}

			data, err := os.ReadFile(filename)
			if err != nil {
				return fmt.Errorf("failed to read file: %w", err)
			}

			kind, err := parseManifestKind(data)
			if err != nil {
				return err
			}

			if isVMKind(kind) {
				return applyVM(cmd, data, dryRun)
			}

			return fmt.Errorf("unsupported resource kind: %q (supported: VM, VirtualMachine)", kind)
		},
	}

	cmd.Flags().StringVarP(&filename, "filename", "f", "", "Filename to apply (required)")
	cmd.Flags().BoolVar(&dryRun, "dry-run", false, "Show what would be applied without applying")
	cmd.MarkFlagRequired("filename")

	return cmd
}

func applyVM(cmd *cobra.Command, data []byte, dryRun bool) error {
	manifest, err := parseVMManifest(data)
	if err != nil {
		return err
	}

	memoryBytes, err := parseMemorySize(manifest.Spec.Memory)
	if err != nil {
		return fmt.Errorf("invalid spec.memory: %w", err)
	}

	enableLogin := true
	if manifest.Spec.EnableKcoreLogin != nil {
		enableLogin = *manifest.Spec.EnableKcoreLogin
	}

	vmID := uuid.New().String()

	var nics []*ctrlpb.Nic
	for _, n := range manifest.Spec.Nics {
		model := n.Model
		if model == "" {
			model = "virtio"
		}
		nics = append(nics, &ctrlpb.Nic{
			Network: n.Network,
			Model:   model,
		})
	}

	spec := &ctrlpb.VmSpec{
		Id:               vmID,
		Name:             manifest.Metadata.Name,
		Cpu:              int32(manifest.Spec.CPU),
		MemoryBytes:      memoryBytes,
		Nics:             nics,
		EnableKcoreLogin: enableLogin,
	}

	imageURI := manifest.Spec.Image
	cloudInit := manifest.Spec.CloudInit

	if dryRun {
		fmt.Printf("Dry run: would create VM from manifest\n\n")
		fmt.Printf("  Name:        %s\n", manifest.Metadata.Name)
		fmt.Printf("  ID:          %s\n", vmID)
		fmt.Printf("  CPU:         %d\n", manifest.Spec.CPU)
		fmt.Printf("  Memory:      %s (%s)\n", manifest.Spec.Memory, formatBytes(memoryBytes))
		if imageURI != "" {
			fmt.Printf("  Image:       %s\n", imageURI)
		}
		fmt.Printf("  Kcore Login: %t\n", enableLogin)
		if len(nics) > 0 {
			for i, n := range nics {
				fmt.Printf("  NIC %d:       %s (%s)\n", i, n.Network, n.Model)
			}
		} else {
			fmt.Printf("  NIC:         default (virtio)\n")
		}
		if cloudInit != "" {
			fmt.Printf("  Cloud-Init:  (custom, %d bytes)\n", len(cloudInit))
		}
		fmt.Printf("\nNo changes made.\n")
		return nil
	}

	root := cmd.Root()
	configPath, _ := root.PersistentFlags().GetString("config")
	controllerAddr, _ := root.PersistentFlags().GetString("controller")
	insecureFlag, _ := root.PersistentFlags().GetBool("insecure")

	ctrlAddr, insecure, certFile, keyFile, caFile, err := GetConnectionInfo(configPath, controllerAddr, insecureFlag)
	if err != nil {
		return err
	}

	fmt.Printf("Applying VM '%s' via controller %s...\n", manifest.Metadata.Name, ctrlAddr)

	client, err := NewControllerVMClient(ctrlAddr, insecure, certFile, keyFile, caFile)
	if err != nil {
		return fmt.Errorf("failed to connect to controller: %w", err)
	}
	defer client.Close()

	timeout := 30 * time.Second
	if imageURI != "" {
		timeout = 10 * time.Minute
	}
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()

	resp, err := client.CreateVM(ctx, spec, imageURI, cloudInit)
	if err != nil {
		return fmt.Errorf("failed to create VM: %w", err)
	}

	fmt.Printf("\nVM '%s' created successfully\n", manifest.Metadata.Name)
	fmt.Printf("  ID:     %s\n", resp.VmId)
	fmt.Printf("  Node:   %s\n", resp.NodeId)
	fmt.Printf("  State:  %s\n", resp.State.String())
	fmt.Printf("  CPU:    %d cores\n", manifest.Spec.CPU)
	fmt.Printf("  Memory: %s\n", manifest.Spec.Memory)
	if imageURI != "" {
		fmt.Printf("  Image:  %s\n", imageURI)
	}
	return nil
}
