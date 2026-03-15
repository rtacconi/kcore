package provider

import (
	"context"
	"fmt"
	"time"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"

	pb "github.com/kcore/kcore/api/controller"
)

func dataSourceNodeNics() *schema.Resource {
	return &schema.Resource{
		ReadContext: dataSourceNodeNicsRead,
		Schema: map[string]*schema.Schema{
			"node_id": {
				Type:        schema.TypeString,
				Required:    true,
				Description: "Node ID to query network interfaces from",
			},
			"interfaces": {
				Type:        schema.TypeList,
				Computed:    true,
				Description: "List of discovered network interfaces",
				Elem: &schema.Resource{
					Schema: map[string]*schema.Schema{
						"name": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Interface name (e.g. eth0, eno1)",
						},
						"mac_address": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "MAC address",
						},
						"ip_address": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Current IP address",
						},
						"is_up": {
							Type:        schema.TypeBool,
							Computed:    true,
							Description: "Whether the interface is up",
						},
						"speed_mbps": {
							Type:        schema.TypeInt,
							Computed:    true,
							Description: "Link speed in Mbps",
						},
						"driver": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Kernel driver name",
						},
						"is_virtual": {
							Type:        schema.TypeBool,
							Computed:    true,
							Description: "Whether this is a virtual interface",
						},
					},
				},
			},
		},
	}
}

func dataSourceNodeNicsRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	client := meta.(*apiClient)
	var diags diag.Diagnostics

	nodeID := d.Get("node_id").(string)

	resp, err := client.controller.ListNetworkInterfaces(ctx, &pb.ListNetworkInterfacesRequest{
		NodeId: nodeID,
	})
	if err != nil {
		return diag.FromErr(fmt.Errorf("ListNetworkInterfaces RPC failed: %w", err))
	}

	interfaces := make([]map[string]interface{}, len(resp.Interfaces))
	for i, iface := range resp.Interfaces {
		interfaces[i] = map[string]interface{}{
			"name":        iface.Name,
			"mac_address": iface.MacAddress,
			"ip_address":  iface.IpAddress,
			"is_up":       iface.IsUp,
			"speed_mbps":  int(iface.SpeedMbps),
			"driver":      iface.Driver,
			"is_virtual":  iface.IsVirtual,
		}
	}

	d.Set("interfaces", interfaces)
	d.SetId(nodeID + "-nics-" + time.Now().UTC().String())

	return diags
}
