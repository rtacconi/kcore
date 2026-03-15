package provider

import (
	"context"
	"fmt"
	"os"
	"path/filepath"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"

	"github.com/kcore/kcore/pkg/pki"
)

func resourceClusterCA() *schema.Resource {
	return &schema.Resource{
		CreateContext: resourceClusterCACreate,
		ReadContext:   resourceClusterCARead,
		DeleteContext: resourceClusterCADelete,
		Schema: map[string]*schema.Schema{
			"cluster_name": {
				Type:        schema.TypeString,
				Required:    true,
				ForceNew:    true,
				Description: "Name of the kcore cluster",
			},
			"base_path": {
				Type:        schema.TypeString,
				Optional:    true,
				ForceNew:    true,
				Description: "Base path for PKI storage (defaults to ~/.kcore/clusters)",
			},
			"controller_ip": {
				Type:        schema.TypeString,
				Optional:    true,
				ForceNew:    true,
				Description: "Controller IP address; when set, also generates controller certs",
			},
			"ca_cert_path": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Path to the generated CA certificate",
			},
			"ca_key_path": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Path to the generated CA key",
			},
			"controller_cert_path": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Path to the generated controller certificate (if controller_ip set)",
			},
			"controller_key_path": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Path to the generated controller key (if controller_ip set)",
			},
		},
	}
}

func resourceClusterCACreate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	clusterName := d.Get("cluster_name").(string)
	basePath := d.Get("base_path").(string)

	mgr, err := pki.NewCAManager(basePath)
	if err != nil {
		return diag.FromErr(fmt.Errorf("failed to create CA manager: %w", err))
	}

	if !mgr.ClusterExists(clusterName) {
		if err := mgr.GenerateCA(clusterName); err != nil {
			return diag.FromErr(fmt.Errorf("failed to generate CA: %w", err))
		}
	}

	clusterDir := filepath.Join(mgr.BasePath, clusterName)
	d.SetId(clusterName)
	d.Set("ca_cert_path", filepath.Join(clusterDir, "ca.crt"))
	d.Set("ca_key_path", filepath.Join(clusterDir, "ca.key"))

	if controllerIP, ok := d.GetOk("controller_ip"); ok && controllerIP.(string) != "" {
		if err := mgr.WriteControllerCerts(clusterName, controllerIP.(string), clusterDir); err != nil {
			return diag.FromErr(fmt.Errorf("failed to generate controller certs: %w", err))
		}
		d.Set("controller_cert_path", filepath.Join(clusterDir, "controller.crt"))
		d.Set("controller_key_path", filepath.Join(clusterDir, "controller.key"))
	}

	return nil
}

func resourceClusterCARead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	clusterName := d.Id()
	basePath := d.Get("base_path").(string)

	mgr, err := pki.NewCAManager(basePath)
	if err != nil {
		return diag.FromErr(fmt.Errorf("failed to create CA manager: %w", err))
	}

	clusterDir := filepath.Join(mgr.BasePath, clusterName)

	if _, err := os.Stat(filepath.Join(clusterDir, "ca.crt")); os.IsNotExist(err) {
		d.SetId("")
		return nil
	}

	d.Set("ca_cert_path", filepath.Join(clusterDir, "ca.crt"))
	d.Set("ca_key_path", filepath.Join(clusterDir, "ca.key"))

	if _, err := os.Stat(filepath.Join(clusterDir, "controller.crt")); err == nil {
		d.Set("controller_cert_path", filepath.Join(clusterDir, "controller.crt"))
		d.Set("controller_key_path", filepath.Join(clusterDir, "controller.key"))
	}

	return nil
}

func resourceClusterCADelete(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	d.SetId("")
	return nil
}
