package provider

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"os"

	"github.com/hashicorp/terraform-plugin-sdk/v2/diag"
	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"

	pb "github.com/kcore/kcore/api/controller"
)

// New returns a new provider instance
func New() *schema.Provider {
	return &schema.Provider{
		Schema: map[string]*schema.Schema{
			"controller_address": {
				Type:        schema.TypeString,
				Required:    true,
				DefaultFunc: schema.EnvDefaultFunc("KCORE_CONTROLLER_ADDRESS", "localhost:9090"),
				Description: "Address of the kcore controller gRPC endpoint",
			},
			"tls_cert_path": {
				Type:        schema.TypeString,
				Optional:    true,
				DefaultFunc: schema.EnvDefaultFunc("KCORE_TLS_CERT", ""),
				Description: "Path to the TLS certificate file",
			},
			"tls_key_path": {
				Type:        schema.TypeString,
				Optional:    true,
				DefaultFunc: schema.EnvDefaultFunc("KCORE_TLS_KEY", ""),
				Description: "Path to the TLS key file",
			},
			"tls_ca_path": {
				Type:        schema.TypeString,
				Optional:    true,
				DefaultFunc: schema.EnvDefaultFunc("KCORE_TLS_CA", ""),
				Description: "Path to the TLS CA certificate file",
			},
			"insecure": {
				Type:        schema.TypeBool,
				Optional:    true,
				DefaultFunc: schema.EnvDefaultFunc("KCORE_INSECURE", false),
				Description: "Disable TLS verification (not recommended for production)",
			},
		},
		ResourcesMap: map[string]*schema.Resource{
			"kcore_vm": resourceVM(),
		},
		DataSourcesMap: map[string]*schema.Resource{
			"kcore_vm":    dataSourceVM(),
			"kcore_node":  dataSourceNode(),
			"kcore_nodes": dataSourceNodes(),
		},
		ConfigureContextFunc: providerConfigure,
	}
}

type apiClient struct {
	conn       *grpc.ClientConn
	controller pb.ControllerClient
}

func providerConfigure(ctx context.Context, d *schema.ResourceData) (interface{}, diag.Diagnostics) {
	var diags diag.Diagnostics

	controllerAddr := d.Get("controller_address").(string)
	tlsCertPath := d.Get("tls_cert_path").(string)
	tlsKeyPath := d.Get("tls_key_path").(string)
	tlsCAPath := d.Get("tls_ca_path").(string)
	insecure := d.Get("insecure").(bool)

	var opts []grpc.DialOption

	if insecure {
		opts = append(opts, grpc.WithInsecure())
	} else if tlsCertPath != "" && tlsKeyPath != "" {
		// Load client cert and key
		cert, err := tls.LoadX509KeyPair(tlsCertPath, tlsKeyPath)
		if err != nil {
			return nil, diag.FromErr(fmt.Errorf("failed to load client cert: %w", err))
		}

		// Create TLS config
		tlsConfig := &tls.Config{
			Certificates: []tls.Certificate{cert},
		}

		// Load CA cert if provided
		if tlsCAPath != "" {
			caCert, err := os.ReadFile(tlsCAPath)
			if err != nil {
				return nil, diag.FromErr(fmt.Errorf("failed to read CA cert: %w", err))
			}

			caCertPool := x509.NewCertPool()
			if !caCertPool.AppendCertsFromPEM(caCert) {
				return nil, diag.FromErr(fmt.Errorf("failed to parse CA cert"))
			}

			tlsConfig.RootCAs = caCertPool
		}

		creds := credentials.NewTLS(tlsConfig)
		opts = append(opts, grpc.WithTransportCredentials(creds))
	} else {
		opts = append(opts, grpc.WithInsecure())
	}

	// Connect to the controller
	conn, err := grpc.Dial(controllerAddr, opts...)
	if err != nil {
		return nil, diag.FromErr(fmt.Errorf("failed to connect to controller: %w", err))
	}

	client := &apiClient{
		conn:       conn,
		controller: pb.NewControllerClient(conn),
	}

	return client, diags
}
