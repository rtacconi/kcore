package main

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/kcore/kcore/pkg/pki"
	"github.com/spf13/cobra"
	"gopkg.in/yaml.v3"
)

func newInitCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "init",
		Short: "Initialize kcore resources",
	}

	cmd.AddCommand(newInitClusterCmd())
	return cmd
}

func newInitClusterCmd() *cobra.Command {
	var (
		clusterName  string
		controllerIP string
		basePath     string
	)

	cmd := &cobra.Command{
		Use:   "cluster",
		Short: "Initialize a new kcore cluster (generate CA and config)",
		Long: `Generate a CA key + certificate for a new cluster and store them locally.

The CA private key never leaves the operator's machine. It is used to sign
certificates for controllers and node-agents.

Example:
  kctl init cluster --name my-cluster --controller-ip 192.168.40.100`,
		RunE: func(cmd *cobra.Command, args []string) error {
			if clusterName == "" {
				return fmt.Errorf("--name is required")
			}

			mgr, err := pki.NewCAManager(basePath)
			if err != nil {
				return fmt.Errorf("create CA manager: %w", err)
			}

			if mgr.ClusterExists(clusterName) {
				return fmt.Errorf("cluster %q already exists at %s", clusterName, filepath.Join(mgr.BasePath, clusterName))
			}

			fmt.Printf("Initializing cluster %q...\n", clusterName)

			if err := mgr.GenerateCA(clusterName); err != nil {
				return fmt.Errorf("generate CA: %w", err)
			}
			fmt.Println("  CA key + certificate generated")

			if controllerIP != "" {
				outDir := filepath.Join(mgr.BasePath, clusterName)
				if err := mgr.WriteControllerCerts(clusterName, controllerIP, outDir); err != nil {
					return fmt.Errorf("generate controller certs: %w", err)
				}
				fmt.Println("  Controller certificate generated")
			}

			clusterDir := filepath.Join(mgr.BasePath, clusterName)
			configPath := filepath.Join(clusterDir, "config.yaml")
			cfg := ClusterConfig{
				ClusterName:   clusterName,
				ControllerIP:  controllerIP,
				CACertPath:    filepath.Join(clusterDir, "ca.crt"),
				ControllerCrt: filepath.Join(clusterDir, "controller.crt"),
				ControllerKey: filepath.Join(clusterDir, "controller.key"),
			}
			data, err := yaml.Marshal(&cfg)
			if err != nil {
				return fmt.Errorf("marshal config: %w", err)
			}
			if err := os.WriteFile(configPath, data, 0644); err != nil {
				return fmt.Errorf("write config: %w", err)
			}
			fmt.Printf("  Config saved to %s\n", configPath)

			fmt.Printf("\nCluster %q initialized at %s\n", clusterName, clusterDir)
			return nil
		},
	}

	cmd.Flags().StringVar(&clusterName, "name", "", "Cluster name (required)")
	cmd.Flags().StringVar(&controllerIP, "controller-ip", "", "Controller IP address for cert generation")
	cmd.Flags().StringVar(&basePath, "base-path", "", "Base path for cluster storage (default: ~/.kcore/clusters)")
	cmd.MarkFlagRequired("name")

	return cmd
}

// ClusterConfig stores per-cluster configuration on the operator machine.
type ClusterConfig struct {
	ClusterName   string `yaml:"clusterName"`
	ControllerIP  string `yaml:"controllerIP,omitempty"`
	CACertPath    string `yaml:"caCertPath"`
	ControllerCrt string `yaml:"controllerCrt,omitempty"`
	ControllerKey string `yaml:"controllerKey,omitempty"`
}
