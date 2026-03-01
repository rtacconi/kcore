package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

func newControlPlaneCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "controlplane",
		Aliases: []string{"cp"},
		Short:   "Unified control-plane operations (admin + automation)",
	}

	cmd.AddCommand(newControlPlaneConfigCmd())
	cmd.AddCommand(newControlPlaneEnrollCmd())
	cmd.AddCommand(newControlPlaneInstallCmd())
	return cmd
}

func newControlPlaneConfigCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "config",
		Short: "Apply NixOS configuration through control-plane API",
	}

	cmd.AddCommand(&cobra.Command{
		Use:   "apply-controller --file <configuration.nix>",
		Short: "Apply configuration on controller host",
		RunE: func(cmd *cobra.Command, args []string) error {
			return fmt.Errorf("not implemented: controlplane config apply-controller")
		},
	})

	cmd.AddCommand(&cobra.Command{
		Use:   "apply-node --node <node-id> --file <configuration.nix>",
		Short: "Apply configuration on target node",
		RunE: func(cmd *cobra.Command, args []string) error {
			return fmt.Errorf("not implemented: controlplane config apply-node")
		},
	})

	return cmd
}

func newControlPlaneEnrollCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "enroll",
		Short: "Enrollment token and bootstrap operations",
	}

	tokenCmd := &cobra.Command{
		Use:   "token",
		Short: "Enrollment token lifecycle",
	}
	tokenCmd.AddCommand(&cobra.Command{
		Use:   "create",
		Short: "Create enrollment token",
		RunE: func(cmd *cobra.Command, args []string) error {
			return fmt.Errorf("not implemented: controlplane enroll token create")
		},
	})
	tokenCmd.AddCommand(&cobra.Command{
		Use:   "revoke --id <token-id>",
		Short: "Revoke enrollment token",
		RunE: func(cmd *cobra.Command, args []string) error {
			return fmt.Errorf("not implemented: controlplane enroll token revoke")
		},
	})
	tokenCmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List enrollment tokens",
		RunE: func(cmd *cobra.Command, args []string) error {
			return fmt.Errorf("not implemented: controlplane enroll token list")
		},
	})

	bootstrapCmd := &cobra.Command{
		Use:   "bootstrap-config --token <token> --hostname <node-hostname>",
		Short: "Fetch bootstrap defaults and CA bundle",
		RunE: func(cmd *cobra.Command, args []string) error {
			return fmt.Errorf("not implemented: controlplane enroll bootstrap-config")
		},
	}

	cmd.AddCommand(tokenCmd)
	cmd.AddCommand(bootstrapCmd)
	return cmd
}

func newControlPlaneInstallCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "install",
		Short: "Install status tracking operations",
	}

	cmd.AddCommand(&cobra.Command{
		Use:   "status get --node <node-id>",
		Short: "Get install status for one node",
		RunE: func(cmd *cobra.Command, args []string) error {
			return fmt.Errorf("not implemented: controlplane install status get")
		},
	})

	cmd.AddCommand(&cobra.Command{
		Use:   "status list",
		Short: "List install statuses",
		RunE: func(cmd *cobra.Command, args []string) error {
			return fmt.Errorf("not implemented: controlplane install status list")
		},
	})

	return cmd
}
