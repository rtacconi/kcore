package provider

import (
	"context"
	"fmt"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"
)

func resourceNodeEnrollment() *schema.Resource {
	return &schema.Resource{
		CreateContext: resourceNodeEnrollmentCreate,
		ReadContext:   resourceNodeEnrollmentRead,
		UpdateContext: resourceNodeEnrollmentUpdate,
		DeleteContext: resourceNodeEnrollmentDelete,
		Schema: map[string]*schema.Schema{
			"enrollment_token": {
				Type:        schema.TypeString,
				Required:    true,
				Sensitive:   true,
				Description: "Enrollment token secret",
			},
			"csr_pem": {
				Type:        schema.TypeString,
				Required:    true,
				Sensitive:   true,
				Description: "Node CSR in PEM format",
			},
			"hostname": {
				Type:        schema.TypeString,
				Required:    true,
				Description: "Node hostname",
			},
			"reported_address": {
				Type:        schema.TypeString,
				Optional:    true,
				Description: "Node reported address",
			},
			"labels": {
				Type:        schema.TypeList,
				Optional:    true,
				Elem:        &schema.Schema{Type: schema.TypeString},
				Description: "Node labels",
			},
			"node_id": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Assigned node ID",
			},
			"signed_cert_pem": {
				Type:        schema.TypeString,
				Computed:    true,
				Sensitive:   true,
				Description: "Signed node certificate PEM",
			},
			"ca_bundle_pem": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "CA bundle PEM",
			},
			"controller_address": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Controller address to use after enrollment",
			},
			"cert_expires_at": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "RFC3339 certificate expiration timestamp",
			},
		},
	}
}

func resourceNodeEnrollmentCreate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return diag.FromErr(fmt.Errorf("not implemented: kcore_node_enrollment create via unified controlplane API"))
}

func resourceNodeEnrollmentRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return diag.FromErr(fmt.Errorf("not implemented: kcore_node_enrollment read via unified controlplane API"))
}

func resourceNodeEnrollmentUpdate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return diag.FromErr(fmt.Errorf("not implemented: kcore_node_enrollment update via unified controlplane API"))
}

func resourceNodeEnrollmentDelete(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return diag.FromErr(fmt.Errorf("not implemented: kcore_node_enrollment delete via unified controlplane API"))
}
