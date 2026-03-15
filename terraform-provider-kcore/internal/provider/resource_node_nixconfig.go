package provider

import (
	"context"
	"fmt"
	"time"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/validation"

	pb "github.com/kcore/kcore/api/controller"
)

func resourceNodeNixConfig() *schema.Resource {
	return &schema.Resource{
		CreateContext: resourceNodeNixConfigCreate,
		ReadContext:   resourceNodeNixConfigRead,
		UpdateContext: resourceNodeNixConfigUpdate,
		DeleteContext: resourceNodeNixConfigDelete,
		Timeouts: &schema.ResourceTimeout{
			Create: schema.DefaultTimeout(15 * time.Minute),
			Update: schema.DefaultTimeout(15 * time.Minute),
		},
		Schema: map[string]*schema.Schema{
			"node_id": {
				Type:        schema.TypeString,
				Required:    true,
				ForceNew:    true,
				Description: "Target node ID",
			},
			"configuration_nix": {
				Type:        schema.TypeString,
				Required:    true,
				Description: "Full NixOS configuration.nix content",
			},
			"rebuild": {
				Type:        schema.TypeBool,
				Optional:    true,
				Default:     true,
				Description: "Trigger nixos-rebuild after updating configuration",
			},
			"strategy": {
				Type:         schema.TypeString,
				Optional:     true,
				Default:      "switch",
				Description:  "Rebuild strategy: switch, boot, test, or dry-build",
				ValidateFunc: validation.StringInSlice([]string{"switch", "boot", "test", "dry-build"}, false),
			},
			"build_output": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Output from the nixos-rebuild command",
			},
		},
	}
}

func resourceNodeNixConfigCreate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	client := meta.(*apiClient)

	nodeID := d.Get("node_id").(string)
	configNix := d.Get("configuration_nix").(string)

	resp, err := client.controller.UpdateNixConfig(ctx, &pb.UpdateNixConfigRequest{
		NodeId:           nodeID,
		ConfigurationNix: configNix,
	})
	if err != nil {
		return diag.FromErr(fmt.Errorf("UpdateNixConfig RPC failed: %w", err))
	}
	if !resp.Success {
		return diag.FromErr(fmt.Errorf("UpdateNixConfig failed: %s", resp.Message))
	}

	d.SetId(nodeID + "-nixconfig")

	if d.Get("rebuild").(bool) {
		strategy := d.Get("strategy").(string)
		rebuildResp, err := client.controller.RebuildNix(ctx, &pb.RebuildNixRequest{
			NodeId:   nodeID,
			Strategy: strategy,
		})
		if err != nil {
			return diag.FromErr(fmt.Errorf("RebuildNix RPC failed: %w", err))
		}
		if !rebuildResp.Success {
			return diag.FromErr(fmt.Errorf("RebuildNix failed: %s", rebuildResp.Message))
		}
		d.Set("build_output", rebuildResp.BuildOutput)
	}

	return nil
}

func resourceNodeNixConfigRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return nil
}

func resourceNodeNixConfigUpdate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	client := meta.(*apiClient)

	nodeID := d.Get("node_id").(string)

	if d.HasChange("configuration_nix") {
		configNix := d.Get("configuration_nix").(string)
		resp, err := client.controller.UpdateNixConfig(ctx, &pb.UpdateNixConfigRequest{
			NodeId:           nodeID,
			ConfigurationNix: configNix,
		})
		if err != nil {
			return diag.FromErr(fmt.Errorf("UpdateNixConfig RPC failed: %w", err))
		}
		if !resp.Success {
			return diag.FromErr(fmt.Errorf("UpdateNixConfig failed: %s", resp.Message))
		}
	}

	if d.Get("rebuild").(bool) {
		strategy := d.Get("strategy").(string)
		rebuildResp, err := client.controller.RebuildNix(ctx, &pb.RebuildNixRequest{
			NodeId:   nodeID,
			Strategy: strategy,
		})
		if err != nil {
			return diag.FromErr(fmt.Errorf("RebuildNix RPC failed: %w", err))
		}
		if !rebuildResp.Success {
			return diag.FromErr(fmt.Errorf("RebuildNix failed: %s", rebuildResp.Message))
		}
		d.Set("build_output", rebuildResp.BuildOutput)
	}

	return nil
}

func resourceNodeNixConfigDelete(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	d.SetId("")
	return nil
}
