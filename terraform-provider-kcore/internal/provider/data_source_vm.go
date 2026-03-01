package provider

import (
	"context"
	"fmt"
	"time"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"

	pb "github.com/kcore/kcore/api/controller"
)

func dataSourceVM() *schema.Resource {
	return &schema.Resource{
		ReadContext: dataSourceVMRead,
		Schema: map[string]*schema.Schema{
			"id": {
				Type:        schema.TypeString,
				Required:    true,
				Description: "VM ID",
			},
			"name": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Name of the VM",
			},
			"cpu": {
				Type:        schema.TypeInt,
				Computed:    true,
				Description: "Number of CPU cores",
			},
			"memory_bytes": {
				Type:        schema.TypeInt,
				Computed:    true,
				Description: "Memory in bytes",
			},
			"disk": {
				Type:        schema.TypeList,
				Computed:    true,
				Description: "Disk configuration",
				Elem: &schema.Resource{
					Schema: map[string]*schema.Schema{
						"name": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Disk name",
						},
						"backend_handle": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Storage backend handle/path",
						},
						"bus": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Disk bus type",
						},
						"device": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Device type",
						},
					},
				},
			},
			"nic": {
				Type:        schema.TypeList,
				Computed:    true,
				Description: "Network interface configuration",
				Elem: &schema.Resource{
					Schema: map[string]*schema.Schema{
						"network": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Network name",
						},
						"model": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "NIC model",
						},
						"mac_address": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "MAC address",
						},
					},
				},
			},
			"state": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Current state of the VM",
			},
			"node_id": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Node ID where the VM is running",
			},
			"created_at": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Creation timestamp",
			},
		},
	}
}

func dataSourceVMRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	client := meta.(*apiClient)
	var diags diag.Diagnostics

	vmID := d.Get("id").(string)

	req := &pb.GetVmRequest{
		VmId: vmID,
	}

	resp, err := client.controller.GetVm(ctx, req)
	if err != nil {
		return diag.FromErr(fmt.Errorf("failed to get VM: %w", err))
	}

	d.SetId(vmID)
	d.Set("name", resp.Spec.Name)
	d.Set("cpu", resp.Spec.Cpu)
	d.Set("memory_bytes", resp.Spec.MemoryBytes)
	d.Set("node_id", resp.NodeId)
	d.Set("state", resp.Status.State.String())

	if resp.Status.CreatedAt != nil {
		d.Set("created_at", resp.Status.CreatedAt.AsTime().Format(time.RFC3339))
	}

	// Set disks
	if len(resp.Spec.Disks) > 0 {
		disks := make([]map[string]interface{}, len(resp.Spec.Disks))
		for i, disk := range resp.Spec.Disks {
			disks[i] = map[string]interface{}{
				"name":           disk.Name,
				"backend_handle": disk.BackendHandle,
				"bus":            disk.Bus,
				"device":         disk.Device,
			}
		}
		d.Set("disk", disks)
	}

	// Set NICs
	if len(resp.Spec.Nics) > 0 {
		nics := make([]map[string]interface{}, len(resp.Spec.Nics))
		for i, nic := range resp.Spec.Nics {
			nics[i] = map[string]interface{}{
				"network":     nic.Network,
				"model":       nic.Model,
				"mac_address": nic.MacAddress,
			}
		}
		d.Set("nic", nics)
	}

	return diags
}
