package provider

import (
	"context"
	"crypto/tls"
	"fmt"
	"time"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/validation"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"

	pb "github.com/kcore/kcore/api/controller"
)

func resourceNodeInstall() *schema.Resource {
	return &schema.Resource{
		CreateContext: resourceNodeInstallCreate,
		ReadContext:   resourceNodeInstallRead,
		DeleteContext: resourceNodeInstallDelete,
		Timeouts: &schema.ResourceTimeout{
			Create: schema.DefaultTimeout(30 * time.Minute),
		},
		Schema: map[string]*schema.Schema{
			"bootstrap_address": {
				Type:        schema.TypeString,
				Required:    true,
				ForceNew:    true,
				Description: "gRPC address of the bootstrap controller (host:port)",
			},
			"insecure": {
				Type:        schema.TypeBool,
				Optional:    true,
				Default:     true,
				ForceNew:    true,
				Description: "Skip TLS verification for bootstrap connection",
			},
			"hostname": {
				Type:         schema.TypeString,
				Optional:     true,
				ForceNew:     true,
				Description:  "Hostname to assign to the installed node",
				ValidateFunc: validation.StringLenBetween(1, 253),
			},
			"root_password": {
				Type:        schema.TypeString,
				Optional:    true,
				ForceNew:    true,
				Sensitive:   true,
				Description: "Root password for the installed node",
			},
			"ssh_keys": {
				Type:        schema.TypeList,
				Optional:    true,
				ForceNew:    true,
				Description: "SSH public keys to authorize",
				Elem:        &schema.Schema{Type: schema.TypeString},
			},
			"run_controller": {
				Type:        schema.TypeBool,
				Optional:    true,
				Default:     false,
				ForceNew:    true,
				Description: "Whether to run the kcore controller on this node",
			},
			"controller_address": {
				Type:        schema.TypeString,
				Optional:    true,
				ForceNew:    true,
				Description: "Controller address for the installed node to join",
			},
			"disk": {
				Type:        schema.TypeList,
				Required:    true,
				ForceNew:    true,
				MinItems:    1,
				Description: "Disk configuration (at least one OS disk required)",
				Elem: &schema.Resource{
					Schema: map[string]*schema.Schema{
						"device": {
							Type:        schema.TypeString,
							Required:    true,
							Description: "Disk device path (e.g. /dev/sda)",
						},
						"role": {
							Type:         schema.TypeString,
							Required:     true,
							Description:  "Disk role: os or storage",
							ValidateFunc: validation.StringInSlice([]string{"os", "storage"}, false),
						},
					},
				},
			},
			"install_id": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Installation ID returned by the controller",
			},
			"status": {
				Type:        schema.TypeString,
				Computed:    true,
				Description: "Installation status",
			},
		},
	}
}

func resourceNodeInstallCreate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	bootstrapAddr := d.Get("bootstrap_address").(string)
	insecure := d.Get("insecure").(bool)

	var opts []grpc.DialOption
	if insecure {
		opts = append(opts, grpc.WithTransportCredentials(credentials.NewTLS(&tls.Config{
			InsecureSkipVerify: true,
		})))
	} else {
		opts = append(opts, grpc.WithTransportCredentials(credentials.NewTLS(&tls.Config{})))
	}

	conn, err := grpc.Dial(bootstrapAddr, opts...)
	if err != nil {
		return diag.FromErr(fmt.Errorf("failed to connect to bootstrap controller %s: %w", bootstrapAddr, err))
	}
	defer conn.Close()

	bootstrapClient := pb.NewControllerClient(conn)

	req := &pb.InstallNodeRequest{
		NodeId:            "", // bootstrap controller assigns
		Hostname:          d.Get("hostname").(string),
		RootPassword:      d.Get("root_password").(string),
		RunController:     d.Get("run_controller").(bool),
		ControllerAddress: d.Get("controller_address").(string),
	}

	if v, ok := d.GetOk("ssh_keys"); ok {
		for _, k := range v.([]interface{}) {
			req.SshKeys = append(req.SshKeys, k.(string))
		}
	}

	if v, ok := d.GetOk("disk"); ok {
		for _, disk := range v.([]interface{}) {
			diskMap := disk.(map[string]interface{})
			req.Disks = append(req.Disks, &pb.DiskConfig{
				Device: diskMap["device"].(string),
				Role:   diskMap["role"].(string),
			})
		}
	}

	resp, err := bootstrapClient.InstallNode(ctx, req)
	if err != nil {
		return diag.FromErr(fmt.Errorf("InstallNode RPC failed: %w", err))
	}

	d.SetId(resp.InstallId)
	d.Set("install_id", resp.InstallId)
	d.Set("status", resp.Status)

	return nil
}

func resourceNodeInstallRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	bootstrapAddr := d.Get("bootstrap_address").(string)
	insecure := d.Get("insecure").(bool)

	var opts []grpc.DialOption
	if insecure {
		opts = append(opts, grpc.WithTransportCredentials(credentials.NewTLS(&tls.Config{
			InsecureSkipVerify: true,
		})))
	} else {
		opts = append(opts, grpc.WithTransportCredentials(credentials.NewTLS(&tls.Config{})))
	}

	conn, err := grpc.Dial(bootstrapAddr, opts...)
	if err != nil {
		return diag.FromErr(fmt.Errorf("failed to connect to bootstrap controller %s: %w", bootstrapAddr, err))
	}
	defer conn.Close()

	bootstrapClient := pb.NewControllerClient(conn)

	resp, err := bootstrapClient.GetInstallStatus(ctx, &pb.GetInstallStatusRequest{
		InstallId: d.Id(),
	})
	if err != nil {
		return diag.FromErr(fmt.Errorf("GetInstallStatus RPC failed: %w", err))
	}

	d.Set("install_id", resp.InstallId)
	d.Set("status", resp.Phase)

	return nil
}

func resourceNodeInstallDelete(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	d.SetId("")
	return nil
}
