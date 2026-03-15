package provider

import (
	"context"
	"fmt"
	"time"

	"github.com/google/uuid"
	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/validation"

	pb "github.com/kcore/kcore/api/controller"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
)

func resourceVM() *schema.Resource {
	return &schema.Resource{
		CreateContext: resourceVMCreate,
		ReadContext:   resourceVMRead,
		UpdateContext: resourceVMUpdate,
		DeleteContext: resourceVMDelete,
		Importer: &schema.ResourceImporter{
			StateContext: schema.ImportStatePassthroughContext,
		},
		Timeouts: &schema.ResourceTimeout{
			Create: schema.DefaultTimeout(10 * time.Minute),
			Update: schema.DefaultTimeout(10 * time.Minute),
			Delete: schema.DefaultTimeout(10 * time.Minute),
		},
		Schema: map[string]*schema.Schema{
			"name": {
				Type:         schema.TypeString,
				Required:     true,
				ForceNew:     true,
				Description:  "Name of the VM",
				ValidateFunc: validation.StringLenBetween(1, 253),
			},
			"cpu": {
				Type:         schema.TypeInt,
				Required:     true,
				Description:  "Number of CPU cores",
				ValidateFunc: validation.IntAtLeast(1),
			},
			"memory_bytes": {
				Type:         schema.TypeInt,
				Required:     true,
				Description:  "Memory in bytes",
				ValidateFunc: validation.IntAtLeast(1024 * 1024), // At least 1MB
			},
			"target_node": {
				Type:        schema.TypeString,
				Optional:    true,
				ForceNew:    true,
				Description: "Target node to create the VM on (optional, controller will schedule if not specified)",
			},
			"disk": {
				Type:        schema.TypeList,
				Optional:    true,
				Description: "Disk configuration",
				Elem: &schema.Resource{
					Schema: map[string]*schema.Schema{
						"name": {
							Type:        schema.TypeString,
							Required:    true,
							Description: "Disk name",
						},
						"backend_handle": {
							Type:        schema.TypeString,
							Required:    true,
							Description: "Storage backend handle/path",
						},
						"bus": {
							Type:         schema.TypeString,
							Optional:     true,
							Default:      "virtio",
							Description:  "Disk bus type (virtio, scsi, ide, sata)",
							ValidateFunc: validation.StringInSlice([]string{"virtio", "scsi", "ide", "sata"}, false),
						},
						"device": {
							Type:        schema.TypeString,
							Optional:    true,
							Default:     "disk",
							Description: "Device type (disk, cdrom)",
						},
					},
				},
			},
			"nic": {
				Type:        schema.TypeList,
				Optional:    true,
				Description: "Network interface configuration",
				Elem: &schema.Resource{
					Schema: map[string]*schema.Schema{
						"network": {
							Type:        schema.TypeString,
							Required:    true,
							Description: "Network name",
						},
						"model": {
							Type:         schema.TypeString,
							Optional:     true,
							Default:      "virtio",
							Description:  "NIC model (virtio, e1000, rtl8139)",
							ValidateFunc: validation.StringInSlice([]string{"virtio", "e1000", "rtl8139"}, false),
						},
						"mac_address": {
							Type:        schema.TypeString,
							Optional:    true,
							Computed:    true,
							Description: "MAC address (auto-generated if not specified)",
						},
					},
				},
			},
			"image_uri": {
				Type:        schema.TypeString,
				Optional:    true,
				ForceNew:    true,
				Description: "HTTP/HTTPS URI to a cloud image (e.g. Debian, Ubuntu qcow2)",
			},
			"enable_kcore_login": {
				Type:        schema.TypeBool,
				Optional:    true,
				Default:     true,
				ForceNew:    true,
				Description: "Enable kcore default credentials via cloud-init (default: true)",
			},
			"cloud_init_user_data": {
				Type:        schema.TypeString,
				Optional:    true,
				ForceNew:    true,
				Description: "Custom cloud-init #cloud-config YAML (overrides default)",
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

func resourceVMCreate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	client := meta.(*apiClient)
	vmID := uuid.NewString()

	spec := &pb.VmSpec{
		Id:               vmID,
		Name:             d.Get("name").(string),
		Cpu:              int32(d.Get("cpu").(int)),
		MemoryBytes:      int64(d.Get("memory_bytes").(int)),
		EnableKcoreLogin: d.Get("enable_kcore_login").(bool),
	}

	// Add disks
	if v, ok := d.GetOk("disk"); ok {
		disks := v.([]interface{})
		for _, disk := range disks {
			diskMap := disk.(map[string]interface{})
			spec.Disks = append(spec.Disks, &pb.Disk{
				Name:          diskMap["name"].(string),
				BackendHandle: diskMap["backend_handle"].(string),
				Bus:           diskMap["bus"].(string),
				Device:        diskMap["device"].(string),
			})
		}
	}

	// Add NICs
	if v, ok := d.GetOk("nic"); ok {
		nics := v.([]interface{})
		for _, nic := range nics {
			nicMap := nic.(map[string]interface{})
			pbNic := &pb.Nic{
				Network: nicMap["network"].(string),
				Model:   nicMap["model"].(string),
			}
			if mac, ok := nicMap["mac_address"].(string); ok && mac != "" {
				pbNic.MacAddress = mac
			}
			spec.Nics = append(spec.Nics, pbNic)
		}
	}

	req := &pb.CreateVmRequest{
		Spec: spec,
	}

	if v, ok := d.GetOk("image_uri"); ok {
		req.ImageUri = v.(string)
	}
	if v, ok := d.GetOk("cloud_init_user_data"); ok {
		req.CloudInitUserData = v.(string)
	}
	if targetNode, ok := d.GetOk("target_node"); ok {
		req.TargetNode = targetNode.(string)
	}

	// Call CreateVM
	resp, err := client.controller.CreateVm(ctx, req)
	if err != nil {
		return diag.FromErr(fmt.Errorf("failed to create VM: %w", err))
	}

	createdID := resp.VmId
	if createdID == "" {
		createdID = vmID
	}
	d.SetId(createdID)
	_ = d.Set("node_id", resp.NodeId)
	_ = d.Set("state", resp.State.String())
	return nil
}

func resourceVMRead(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	client := meta.(*apiClient)
	var diags diag.Diagnostics

	vmID := d.Id()

	req := &pb.GetVmRequest{
		VmId: vmID,
	}

	resp, err := client.controller.GetVm(ctx, req)
	if err != nil {
		if status.Code(err) == codes.NotFound {
			// VM not found, remove from state.
			d.SetId("")
			return diags
		}
		return diag.FromErr(fmt.Errorf("failed to read VM %s: %w", vmID, err))
	}

	// Set computed fields
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

func resourceVMUpdate(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	// For now, most changes require recreation (ForceNew)
	// In the future, you could implement live updates for certain fields like CPU/memory
	return resourceVMRead(ctx, d, meta)
}

func resourceVMDelete(ctx context.Context, d *schema.ResourceData, meta interface{}) diag.Diagnostics {
	client := meta.(*apiClient)
	var diags diag.Diagnostics

	vmID := d.Id()

	req := &pb.DeleteVmRequest{
		VmId: vmID,
	}

	_, err := client.controller.DeleteVm(ctx, req)
	if err != nil {
		return diag.FromErr(fmt.Errorf("failed to delete VM: %w", err))
	}

	d.SetId("")
	return diags
}
