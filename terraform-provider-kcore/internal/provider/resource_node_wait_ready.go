package provider

import (
	"context"
	"fmt"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"
)

func resourceNodeWaitReady() *schema.Resource {
	return &schema.Resource{
		CreateContext: resourceNodeWaitReadyCreate,
		ReadContext:   resourceNodeWaitReadyRead,
		UpdateContext: resourceNodeWaitReadyUpdate,
		DeleteContext: resourceNodeWaitReadyDelete,
		Schema: map[string]*schema.Schema{
			"node_id": {
				Type:        schema.TypeString,
				Required:    true,
				Description: "Node ID to wait for",
			},
			"timeout_seconds": {
				Type:        schema.TypeInt,
				Optional:    true,
				Default:     300,
				Description: "Maximum time to wait for node ready",
			},
			"status": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Observed node status",
			},
		},
	}
}

func resourceNodeWaitReadyCreate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return diag.FromErr(fmt.Errorf("not implemented: kcore_node_wait_ready create via unified controlplane API"))
}

func resourceNodeWaitReadyRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return diag.FromErr(fmt.Errorf("not implemented: kcore_node_wait_ready read via unified controlplane API"))
}

func resourceNodeWaitReadyUpdate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return diag.FromErr(fmt.Errorf("not implemented: kcore_node_wait_ready update via unified controlplane API"))
}

func resourceNodeWaitReadyDelete(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return nil
}
