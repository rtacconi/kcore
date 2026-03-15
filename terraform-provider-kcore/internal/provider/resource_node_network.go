package provider

import (
	"context"
	"fmt"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"

	pb "github.com/kcore/kcore/api/controller"
)

func resourceNodeNetwork() *schema.Resource {
	return &schema.Resource{
		CreateContext: resourceNodeNetworkCreate,
		ReadContext:   resourceNodeNetworkRead,
		DeleteContext: resourceNodeNetworkDelete,
		Schema: map[string]*schema.Schema{
			"node_id": {
				Type:        schema.TypeString,
				Required:    true,
				ForceNew:    true,
				Description: "Target node ID",
			},
			"dns_servers": {
				Type:        schema.TypeString,
				Optional:    true,
				ForceNew:    true,
				Description: "Comma-separated DNS server addresses",
			},
			"apply_now": {
				Type:        schema.TypeBool,
				Optional:    true,
				Default:     false,
				ForceNew:    true,
				Description: "Apply network configuration immediately (triggers nixos-rebuild)",
			},
			"bridge": {
				Type:        schema.TypeList,
				Optional:    true,
				ForceNew:    true,
				Description: "Bridge interface configuration",
				Elem: &schema.Resource{
					Schema: map[string]*schema.Schema{
						"name": {
							Type:        schema.TypeString,
							Required:    true,
							Description: "Bridge interface name",
						},
						"member_ports": {
							Type:        schema.TypeList,
							Required:    true,
							Description: "Physical ports to include in the bridge",
							Elem:        &schema.Schema{Type: schema.TypeString},
						},
						"dhcp": {
							Type:        schema.TypeBool,
							Optional:    true,
							Default:     false,
							Description: "Use DHCP for address assignment",
						},
						"ip_address": {
							Type:        schema.TypeString,
							Optional:    true,
							Description: "Static IP address",
						},
						"subnet_mask": {
							Type:        schema.TypeString,
							Optional:    true,
							Description: "Subnet mask",
						},
						"gateway": {
							Type:        schema.TypeString,
							Optional:    true,
							Description: "Default gateway",
						},
					},
				},
			},
			"bond": {
				Type:        schema.TypeList,
				Optional:    true,
				ForceNew:    true,
				Description: "Bond interface configuration",
				Elem: &schema.Resource{
					Schema: map[string]*schema.Schema{
						"name": {
							Type:        schema.TypeString,
							Required:    true,
							Description: "Bond interface name",
						},
						"member_ports": {
							Type:        schema.TypeList,
							Required:    true,
							Description: "Physical ports to bond",
							Elem:        &schema.Schema{Type: schema.TypeString},
						},
						"mode": {
							Type:        schema.TypeString,
							Optional:    true,
							Default:     "802.3ad",
							Description: "Bond mode (e.g. 802.3ad, active-backup)",
						},
						"ip_address": {
							Type:        schema.TypeString,
							Optional:    true,
							Description: "Static IP address",
						},
						"subnet_mask": {
							Type:        schema.TypeString,
							Optional:    true,
							Description: "Subnet mask",
						},
						"dhcp": {
							Type:        schema.TypeBool,
							Optional:    true,
							Default:     false,
							Description: "Use DHCP for address assignment",
						},
					},
				},
			},
			"vlan": {
				Type:        schema.TypeList,
				Optional:    true,
				ForceNew:    true,
				Description: "VLAN interface configuration",
				Elem: &schema.Resource{
					Schema: map[string]*schema.Schema{
						"parent_interface": {
							Type:        schema.TypeString,
							Required:    true,
							Description: "Parent interface for the VLAN",
						},
						"vlan_id": {
							Type:        schema.TypeInt,
							Required:    true,
							Description: "VLAN ID (1-4094)",
						},
						"ip_address": {
							Type:        schema.TypeString,
							Optional:    true,
							Description: "Static IP address",
						},
						"subnet_mask": {
							Type:        schema.TypeString,
							Optional:    true,
							Description: "Subnet mask",
						},
						"dhcp": {
							Type:        schema.TypeBool,
							Optional:    true,
							Default:     false,
							Description: "Use DHCP for address assignment",
						},
					},
				},
			},
			"generated_nix_snippet": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Generated NixOS network configuration snippet",
			},
		},
	}
}

func resourceNodeNetworkCreate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	client := meta.(*apiClient)

	nodeID := d.Get("node_id").(string)

	req := &pb.ConfigureNetworkRequest{
		NodeId:     nodeID,
		DnsServers: d.Get("dns_servers").(string),
		ApplyNow:   d.Get("apply_now").(bool),
	}

	if v, ok := d.GetOk("bridge"); ok {
		for _, b := range v.([]interface{}) {
			bm := b.(map[string]interface{})
			bridge := &pb.BridgeConfig{
				Name:       bm["name"].(string),
				Dhcp:       bm["dhcp"].(bool),
				IpAddress:  bm["ip_address"].(string),
				SubnetMask: bm["subnet_mask"].(string),
				Gateway:    bm["gateway"].(string),
			}
			for _, p := range bm["member_ports"].([]interface{}) {
				bridge.MemberPorts = append(bridge.MemberPorts, p.(string))
			}
			req.Bridges = append(req.Bridges, bridge)
		}
	}

	if v, ok := d.GetOk("bond"); ok {
		for _, b := range v.([]interface{}) {
			bm := b.(map[string]interface{})
			bond := &pb.BondConfig{
				Name:       bm["name"].(string),
				Mode:       bm["mode"].(string),
				Dhcp:       bm["dhcp"].(bool),
				IpAddress:  bm["ip_address"].(string),
				SubnetMask: bm["subnet_mask"].(string),
			}
			for _, p := range bm["member_ports"].([]interface{}) {
				bond.MemberPorts = append(bond.MemberPorts, p.(string))
			}
			req.Bonds = append(req.Bonds, bond)
		}
	}

	if v, ok := d.GetOk("vlan"); ok {
		for _, vl := range v.([]interface{}) {
			vm := vl.(map[string]interface{})
			req.Vlans = append(req.Vlans, &pb.VlanConfig{
				ParentInterface: vm["parent_interface"].(string),
				VlanId:          int32(vm["vlan_id"].(int)),
				Dhcp:            vm["dhcp"].(bool),
				IpAddress:       vm["ip_address"].(string),
				SubnetMask:      vm["subnet_mask"].(string),
			})
		}
	}

	resp, err := client.controller.ConfigureNetwork(ctx, req)
	if err != nil {
		return diag.FromErr(fmt.Errorf("ConfigureNetwork RPC failed: %w", err))
	}

	if !resp.Success {
		return diag.FromErr(fmt.Errorf("ConfigureNetwork failed: %s", resp.Message))
	}

	d.SetId(nodeID + "-network")
	d.Set("generated_nix_snippet", resp.GeneratedNixSnippet)

	return nil
}

func resourceNodeNetworkRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return nil
}

func resourceNodeNetworkDelete(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	d.SetId("")
	return nil
}
