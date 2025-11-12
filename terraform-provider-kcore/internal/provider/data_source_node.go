package provider

import (
	"context"
	"fmt"
	"time"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"

	pb "github.com/kcore/kcore/api/controller"
)

func dataSourceNode() *schema.Resource {
	return &schema.Resource{
		ReadContext: dataSourceNodeRead,
		Schema: map[string]*schema.Schema{
			"id": {
				Type:        schema.TypeString,
				Required:    true,
				Description: "Node ID",
			},
			"hostname": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Node hostname",
			},
			"address": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Node gRPC address",
			},
			"status": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Node status (ready, not-ready, unknown)",
			},
			"cpu_cores": {
				Type:        schema.TypeInt,
				Computed:    true,
				Description: "Total CPU cores",
			},
			"memory_bytes": {
				Type:        schema.TypeInt,
				Computed:    true,
				Description: "Total memory in bytes",
			},
			"cpu_cores_used": {
				Type:        schema.TypeInt,
				Computed:    true,
				Description: "Used CPU cores",
			},
			"memory_bytes_used": {
				Type:        schema.TypeInt,
				Computed:    true,
				Description: "Used memory in bytes",
			},
			"last_heartbeat": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Last heartbeat timestamp",
			},
		},
	}
}

func dataSourceNodeRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	client := meta.(*apiClient)
	var diags diag.Diagnostics

	nodeID := d.Get("id").(string)

	req := &pb.GetNodeRequest{
		NodeId: nodeID,
	}

	resp, err := client.controller.GetNode(ctx, req)
	if err != nil {
		return diag.FromErr(fmt.Errorf("failed to get node: %w", err))
	}

	node := resp.Node
	d.SetId(node.NodeId)
	d.Set("hostname", node.Hostname)
	d.Set("address", node.Address)
	d.Set("status", node.Status)

	if node.Capacity != nil {
		d.Set("cpu_cores", node.Capacity.CpuCores)
		d.Set("memory_bytes", node.Capacity.MemoryBytes)
	}

	if node.Usage != nil {
		d.Set("cpu_cores_used", node.Usage.CpuCoresUsed)
		d.Set("memory_bytes_used", node.Usage.MemoryBytesUsed)
	}

	if node.LastHeartbeat != nil {
		d.Set("last_heartbeat", node.LastHeartbeat.AsTime().Format(time.RFC3339))
	}

	return diags
}

func dataSourceNodes() *schema.Resource {
	return &schema.Resource{
		ReadContext: dataSourceNodesRead,
		Schema: map[string]*schema.Schema{
			"nodes": {
				Type:        schema.TypeList,
				Computed:    true,
				Description: "List of nodes",
				Elem: &schema.Resource{
					Schema: map[string]*schema.Schema{
						"id": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Node ID",
						},
						"hostname": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Node hostname",
						},
						"address": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Node gRPC address",
						},
						"status": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Node status",
						},
						"cpu_cores": {
							Type:        schema.TypeInt,
							Computed:    true,
							Description: "Total CPU cores",
						},
						"memory_bytes": {
							Type:        schema.TypeInt,
							Computed:    true,
							Description: "Total memory in bytes",
						},
						"cpu_cores_used": {
							Type:        schema.TypeInt,
							Computed:    true,
							Description: "Used CPU cores",
						},
						"memory_bytes_used": {
							Type:        schema.TypeInt,
							Computed:    true,
							Description: "Used memory in bytes",
						},
						"last_heartbeat": {
							Type:        schema.TypeString,
							Computed:    true,
							Description: "Last heartbeat timestamp",
						},
					},
				},
			},
		},
	}
}

func dataSourceNodesRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	client := meta.(*apiClient)
	var diags diag.Diagnostics

	req := &pb.ListNodesRequest{}

	resp, err := client.controller.ListNodes(ctx, req)
	if err != nil {
		return diag.FromErr(fmt.Errorf("failed to list nodes: %w", err))
	}

	nodes := make([]map[string]interface{}, len(resp.Nodes))
	for i, node := range resp.Nodes {
		nodeMap := map[string]interface{}{
			"id":       node.NodeId,
			"hostname": node.Hostname,
			"address":  node.Address,
			"status":   node.Status,
		}

		if node.Capacity != nil {
			nodeMap["cpu_cores"] = node.Capacity.CpuCores
			nodeMap["memory_bytes"] = node.Capacity.MemoryBytes
		}

		if node.Usage != nil {
			nodeMap["cpu_cores_used"] = node.Usage.CpuCoresUsed
			nodeMap["memory_bytes_used"] = node.Usage.MemoryBytesUsed
		}

		if node.LastHeartbeat != nil {
			nodeMap["last_heartbeat"] = node.LastHeartbeat.AsTime().Format(time.RFC3339)
		}

		nodes[i] = nodeMap
	}

	d.Set("nodes", nodes)
	d.SetId(time.Now().UTC().String())

	return diags
}
