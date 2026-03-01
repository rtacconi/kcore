package provider

import (
	"context"
	"fmt"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"
)

func resourceEnrollmentToken() *schema.Resource {
	return &schema.Resource{
		CreateContext: resourceEnrollmentTokenCreate,
		ReadContext:   resourceEnrollmentTokenRead,
		UpdateContext: resourceEnrollmentTokenUpdate,
		DeleteContext: resourceEnrollmentTokenDelete,
		Schema: map[string]*schema.Schema{
			"scope": {
				Type:        schema.TypeString,
				Required:    true,
				Description: "Enrollment token scope (e.g. NODE_BOOTSTRAP)",
			},
			"description": {
				Type:        schema.TypeString,
				Optional:    true,
				Description: "Human-readable token description",
			},
			"allowed_labels": {
				Type:        schema.TypeList,
				Optional:    true,
				Elem:        &schema.Schema{Type: schema.TypeString},
				Description: "Optional list of labels allowed for enrollment",
			},
			"expires_at": {
				Type:        schema.TypeString,
				Optional:    true,
				Description: "RFC3339 expiration timestamp",
			},
			"token_secret": {
				Type:        schema.TypeString,
				Computed:    true,
				Sensitive:   true,
				Description: "One-time enrollment secret",
			},
			"revoked": {
				Type:        schema.TypeBool,
				Computed:    true,
				Description: "Whether token was revoked",
			},
		},
	}
}

func resourceEnrollmentTokenCreate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return diag.FromErr(fmt.Errorf("not implemented: kcore_enrollment_token create via unified controlplane API"))
}

func resourceEnrollmentTokenRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return diag.FromErr(fmt.Errorf("not implemented: kcore_enrollment_token read via unified controlplane API"))
}

func resourceEnrollmentTokenUpdate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return diag.FromErr(fmt.Errorf("not implemented: kcore_enrollment_token update via unified controlplane API"))
}

func resourceEnrollmentTokenDelete(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	return diag.FromErr(fmt.Errorf("not implemented: kcore_enrollment_token delete via unified controlplane API"))
}
