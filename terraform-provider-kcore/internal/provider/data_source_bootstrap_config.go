package provider

import (
	"context"
	"fmt"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"
)

func dataSourceBootstrapConfig() *schema.Resource {
	return &schema.Resource{
		ReadContext: dataSourceBootstrapConfigRead,
		Schema: map[string]*schema.Schema{
			"enrollment_token": {
				Type:        schema.TypeString,
				Required:    true,
				Sensitive:   true,
				Description: "Enrollment token secret",
			},
			"node_hostname": {
				Type:        schema.TypeString,
				Required:    true,
				Description: "Node hostname requesting bootstrap configuration",
			},
			"controller_address": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Controller address from bootstrap API",
			},
			"ca_bundle_pem": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "CA bundle from bootstrap API",
			},
		},
	}
}

func dataSourceBootstrapConfigRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return diag.FromErr(fmt.Errorf("not implemented: data source kcore_bootstrap_config via unified controlplane API"))
}
