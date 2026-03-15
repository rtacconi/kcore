package provider

import (
	"context"
	"fmt"
	"time"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"

	pb "github.com/kcore/kcore/api/controller"
)

func dataSourceNodeDisks() *schema.Resource {
	return &schema.Resource{
		ReadContext: dataSourceNodeDisksRead,
		Schema: map[string]*schema.Schema{
			"node_id": {
				Type:        schema.TypeString,
				Required:    true,
				Description: "Node ID to query disks from",
			},
			"bootstrap_mode": {
				Type:        schema.TypeBool,
				Computed:    true,
				Description: "Whether the node is in bootstrap mode",
			},
			"disks": {
				Type:        schema.TypeList,
				Computed:    true,
				Description: "List of discovered disks",
				Elem: &schema.Resource{
					Schema: map[string]*schema.Schema{
						"name": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Disk device name (e.g. sda)",
						},
						"size_bytes": {
							Type:        schema.TypeInt,
							Computed:    true,
							Description: "Disk size in bytes",
						},
						"model": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Disk model string",
						},
						"serial": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Disk serial number",
						},
						"removable": {
							Type:        schema.TypeBool,
							Computed:    true,
							Description: "Whether the disk is removable",
						},
						"has_partitions": {
							Type:        schema.TypeBool,
							Computed:    true,
							Description: "Whether the disk has existing partitions",
						},
					},
				},
			},
		},
	}
}

func dataSourceNodeDisksRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	client := meta.(*apiClient)
	var diags diag.Diagnostics

	nodeID := d.Get("node_id").(string)

	resp, err := client.controller.ListNodeDisks(ctx, &pb.ListNodeDisksRequest{
		NodeId: nodeID,
	})
	if err != nil {
		return diag.FromErr(fmt.Errorf("ListNodeDisks RPC failed: %w", err))
	}

	disks := make([]map[string]interface{}, len(resp.Disks))
	for i, disk := range resp.Disks {
		disks[i] = map[string]interface{}{
			"name":           disk.Name,
			"size_bytes":     int(disk.SizeBytes),
			"model":          disk.Model,
			"serial":         disk.Serial,
			"removable":      disk.Removable,
			"has_partitions": disk.HasPartitions,
		}
	}

	d.Set("disks", disks)
	d.Set("bootstrap_mode", resp.BootstrapMode)
	d.SetId(nodeID + "-disks-" + time.Now().UTC().String())

	return diags
}
