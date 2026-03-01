package ovn

import (
	"bytes"
	"context"
	"fmt"
	"log"
	"os/exec"
	"strings"
)

// Client is a very small wrapper around the ovn-nbctl CLI for configuring
// OVN logical switches, DHCP options, and logical ports on a single node.
//
// This deliberately avoids schema-specific libovsdb models so we can keep
// the integration light-weight while still taking advantage of OVN's native
// DHCP. In the future this can be migrated to libovsdb models without
// changing the call sites.
type Client struct {
	switchName string
	cidr       string
	routerIP   string
}

// NewClient returns an OVN client bound to the given logical switch and
// subnet. It checks that ovn-nbctl is available; if not, it returns nil
// without error so callers can gracefully disable OVN integration.
func NewClient(switchName, cidr, routerIP string) *Client {
	if _, err := exec.LookPath("ovn-nbctl"); err != nil {
		log.Printf("OVN: ovn-nbctl not found on PATH; OVN integration disabled (%v)", err)
		return nil
	}
	return &Client{
		switchName: switchName,
		cidr:       cidr,
		routerIP:   routerIP,
	}
}

// run executes ovn-nbctl with the given arguments and returns stdout.
func (c *Client) run(ctx context.Context, args ...string) (string, error) {
	if c == nil {
		return "", nil
	}
	cmd := exec.CommandContext(ctx, "ovn-nbctl", args...)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return "", fmt.Errorf("ovn-nbctl %v failed: %w (stderr: %s)", args, err, strings.TrimSpace(stderr.String()))
	}
	return strings.TrimSpace(stdout.String()), nil
}

// EnsureNetwork ensures that a logical switch and DHCP options exist for the
// configured CIDR. It is safe to call this multiple times.
func (c *Client) EnsureNetwork(ctx context.Context) error {
	if c == nil {
		return nil
	}

	// 1) Ensure logical switch exists.
	if _, err := c.run(ctx, "--may-exist", "ls-add", c.switchName); err != nil {
		return fmt.Errorf("OVN: failed to ensure logical switch %s: %w", c.switchName, err)
	}

	// 2) Find existing DHCP_Options for this CIDR.
	dhcpUUID, err := c.run(ctx, "--bare", "--columns=_uuid", "find", "DHCP_Options", fmt.Sprintf("cidr==\"%s\"", c.cidr))
	if err != nil {
		return fmt.Errorf("OVN: failed to query DHCP_Options: %w", err)
	}

	if dhcpUUID == "" {
		// Create a new DHCP_Options row.
		dhcpUUID, err = c.run(ctx, "create", "DHCP_Options",
			fmt.Sprintf("cidr=%s", c.cidr),
			fmt.Sprintf("options:router=\"%s\"", c.routerIP),
			fmt.Sprintf("options:server_id=\"%s\"", c.routerIP),
			"options:server_mac=\"00:00:00:aa:bb:cc\"",
			"options:lease_time=\"3600\"",
		)
		if err != nil {
			return fmt.Errorf("OVN: failed to create DHCP_Options for %s: %w", c.cidr, err)
		}
		log.Printf("OVN: created DHCP_Options %s for %s", dhcpUUID, c.cidr)
	}

	// 3) Attach DHCP options and subnet metadata to the logical switch.
	if _, err := c.run(ctx, "set", "Logical_Switch", c.switchName,
		fmt.Sprintf("other_config:subnet=\"%s\"", c.cidr),
		fmt.Sprintf("other_config:exclude_ips=\"%s\"", c.routerIP),
		fmt.Sprintf("dhcpv4_options=%s", dhcpUUID),
	); err != nil {
		return fmt.Errorf("OVN: failed to attach DHCP options to switch %s: %w", c.switchName, err)
	}

	return nil
}

// EnsurePortForVM ensures there is a logical switch port for the given VM
// NIC ifaceID on the configured logical switch. The port is configured to
// use dynamic addressing so OVN's native DHCP will assign an IP.
func (c *Client) EnsurePortForVM(ctx context.Context, ifaceID, mac string) error {
	if c == nil {
		return nil
	}

	// 1) Create logical switch port (idempotent).
	if _, err := c.run(ctx, "--may-exist", "lsp-add", c.switchName, ifaceID); err != nil {
		return fmt.Errorf("OVN: failed to create logical switch port %s: %w", ifaceID, err)
	}

	// 2) Configure addresses for DHCP.
	addr := "dynamic"
	if mac != "" {
		addr = fmt.Sprintf("%s dynamic", mac)
	}
	if _, err := c.run(ctx, "lsp-set-addresses", ifaceID, addr); err != nil {
		return fmt.Errorf("OVN: failed to set addresses for port %s: %w", ifaceID, err)
	}

	log.Printf("OVN: logical port %s on %s configured with addresses=%q", ifaceID, c.switchName, addr)
	return nil
}

