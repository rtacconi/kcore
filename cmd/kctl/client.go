package main

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"os"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"

	ctrlpb "github.com/kcore/kcore/api/controller"
	pb "github.com/kcore/kcore/api/node"
	nodepb "github.com/kcore/kcore/api/node"
)

// NodeClient wraps gRPC client for node operations
type NodeClient struct {
	conn    *grpc.ClientConn
	compute nodepb.NodeComputeClient
	storage nodepb.NodeStorageClient
	info    nodepb.NodeInfoClient
}

// NewNodeClient creates a new node client
func NewNodeClient(address string, tlsInsecure bool, certFile, keyFile, caFile string) (*NodeClient, error) {
	var opts []grpc.DialOption

	if certFile != "" && keyFile != "" {
		// Load client cert
		cert, err := tls.LoadX509KeyPair(certFile, keyFile)
		if err != nil {
			return nil, fmt.Errorf("failed to load client cert: %w", err)
		}

		// Load CA cert if provided
		var certPool *x509.CertPool
		if caFile != "" {
			caCert, err := os.ReadFile(caFile)
			if err != nil {
				return nil, fmt.Errorf("failed to read CA cert: %w", err)
			}
			certPool = x509.NewCertPool()
			if !certPool.AppendCertsFromPEM(caCert) {
				return nil, fmt.Errorf("failed to append CA cert")
			}
		}

		tlsConfig := &tls.Config{
			Certificates:       []tls.Certificate{cert},
			RootCAs:            certPool,
			InsecureSkipVerify: tlsInsecure,
			MinVersion:         tls.VersionTLS12,
		}

		opts = append(opts, grpc.WithTransportCredentials(credentials.NewTLS(tlsConfig)))
	} else {
		// No certs provided: use TLS but skip verification
		tlsConfig := &tls.Config{
			InsecureSkipVerify: true,
			MinVersion:         tls.VersionTLS12,
		}
		opts = append(opts, grpc.WithTransportCredentials(credentials.NewTLS(tlsConfig)))
	}

	// Add timeout
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	conn, err := grpc.DialContext(ctx, address, opts...)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to %s: %w", address, err)
	}

	return &NodeClient{
		conn:    conn,
		compute: nodepb.NewNodeComputeClient(conn),
		storage: nodepb.NewNodeStorageClient(conn),
		info:    nodepb.NewNodeInfoClient(conn),
	}, nil
}

// Close closes the client connection
func (c *NodeClient) Close() error {
	if c.conn != nil {
		return c.conn.Close()
	}
	return nil
}

// CreateVM creates a new VM on the node
func (c *NodeClient) CreateVM(ctx context.Context, spec *pb.VmSpec, imageURI, imagePath string) (*pb.VmStatus, error) {
	resp, err := c.compute.CreateVm(ctx, &nodepb.CreateVmRequest{
		Spec:      spec,
		ImageUri:  imageURI,
		ImagePath: imagePath,
	})
	if err != nil {
		return nil, err
	}
	return resp.Status, nil
}

// GetVM gets VM details
func (c *NodeClient) GetVM(ctx context.Context, vmID string) (*pb.VmSpec, *pb.VmStatus, error) {
	resp, err := c.compute.GetVm(ctx, &nodepb.GetVmRequest{
		VmId: vmID,
	})
	if err != nil {
		return nil, nil, err
	}
	return resp.Spec, resp.Status, nil
}

// ListVMs lists all VMs on the node
func (c *NodeClient) ListVMs(ctx context.Context) ([]*pb.VmInfo, error) {
	resp, err := c.compute.ListVms(ctx, &nodepb.ListVmsRequest{})
	if err != nil {
		return nil, err
	}
	return resp.Vms, nil
}

// DeleteVM deletes a VM
func (c *NodeClient) DeleteVM(ctx context.Context, vmID string) error {
	_, err := c.compute.DeleteVm(ctx, &nodepb.DeleteVmRequest{
		VmId: vmID,
	})
	return err
}

// GetNodeInfo gets node information
func (c *NodeClient) GetNodeInfo(ctx context.Context) (*pb.GetNodeInfoResponse, error) {
	return c.info.GetNodeInfo(ctx, &nodepb.GetNodeInfoRequest{})
}

// ControllerAdminClient is a thin wrapper for controller admin RPCs
type ControllerAdminClient struct {
	conn *grpc.ClientConn
	api  ctrlpb.ControllerAdminClient
}

func NewControllerAdminClient(address string, tlsInsecure bool, certFile, keyFile, caFile string) (*ControllerAdminClient, error) {
	var opts []grpc.DialOption

	if certFile != "" && keyFile != "" {
		cert, err := tls.LoadX509KeyPair(certFile, keyFile)
		if err != nil {
			return nil, fmt.Errorf("failed to load client cert: %w", err)
		}

		var certPool *x509.CertPool
		if caFile != "" {
			caCert, err := os.ReadFile(caFile)
			if err != nil {
				return nil, fmt.Errorf("failed to read CA cert: %w", err)
			}
			certPool = x509.NewCertPool()
			if !certPool.AppendCertsFromPEM(caCert) {
				return nil, fmt.Errorf("failed to append CA cert")
			}
		}

		tlsConfig := &tls.Config{
			Certificates:       []tls.Certificate{cert},
			RootCAs:            certPool,
			InsecureSkipVerify: tlsInsecure,
			MinVersion:         tls.VersionTLS12,
		}
		opts = append(opts, grpc.WithTransportCredentials(credentials.NewTLS(tlsConfig)))
	} else {
		tlsConfig := &tls.Config{
			InsecureSkipVerify: true,
			MinVersion:         tls.VersionTLS12,
		}
		opts = append(opts, grpc.WithTransportCredentials(credentials.NewTLS(tlsConfig)))
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	conn, err := grpc.DialContext(ctx, address, opts...)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to %s: %w", address, err)
	}

	return &ControllerAdminClient{
		conn: conn,
		api:  ctrlpb.NewControllerAdminClient(conn),
	}, nil
}

func (c *ControllerAdminClient) Close() error {
	if c.conn != nil {
		return c.conn.Close()
	}
	return nil
}

func (c *ControllerAdminClient) ApplyNixConfig(ctx context.Context, config string, rebuild bool) (*ctrlpb.ApplyNixConfigResponse, error) {
	return c.api.ApplyNixConfig(ctx, &ctrlpb.ApplyNixConfigRequest{
		ConfigurationNix: config,
		Rebuild:          rebuild,
	})
}

// NodeAdminClient is a thin wrapper for node admin RPCs
type NodeAdminClient struct {
	conn *grpc.ClientConn
	api  nodepb.NodeAdminClient
}

// NewNodeAdminClient creates a new NodeAdmin client using the same TLS logic
// as NewNodeClient.
func NewNodeAdminClient(address string, tlsInsecure bool, certFile, keyFile, caFile string) (*NodeAdminClient, error) {
	var opts []grpc.DialOption

	if certFile != "" && keyFile != "" {
		// Load client cert
		cert, err := tls.LoadX509KeyPair(certFile, keyFile)
		if err != nil {
			return nil, fmt.Errorf("failed to load client cert: %w", err)
		}

		// Load CA cert if provided
		var certPool *x509.CertPool
		if caFile != "" {
			caCert, err := os.ReadFile(caFile)
			if err != nil {
				return nil, fmt.Errorf("failed to read CA cert: %w", err)
			}
			certPool = x509.NewCertPool()
			if !certPool.AppendCertsFromPEM(caCert) {
				return nil, fmt.Errorf("failed to append CA cert")
			}
		}

		tlsConfig := &tls.Config{
			Certificates:       []tls.Certificate{cert},
			RootCAs:            certPool,
			InsecureSkipVerify: tlsInsecure,
			MinVersion:         tls.VersionTLS12,
		}

		opts = append(opts, grpc.WithTransportCredentials(credentials.NewTLS(tlsConfig)))
	} else {
		// No certs provided: use TLS but skip verification (dev convenience)
		tlsConfig := &tls.Config{
			InsecureSkipVerify: true,
			MinVersion:         tls.VersionTLS12,
		}
		opts = append(opts, grpc.WithTransportCredentials(credentials.NewTLS(tlsConfig)))
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	conn, err := grpc.DialContext(ctx, address, opts...)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to %s: %w", address, err)
	}

	return &NodeAdminClient{
		conn: conn,
		api:  nodepb.NewNodeAdminClient(conn),
	}, nil
}

// Close closes the NodeAdmin client connection.
func (c *NodeAdminClient) Close() error {
	if c.conn != nil {
		return c.conn.Close()
	}
	return nil
}

// ApplyNixConfig pushes a NixOS configuration to the node.
func (c *NodeAdminClient) ApplyNixConfig(ctx context.Context, config string, rebuild bool) (*nodepb.ApplyNixConfigResponse, error) {
	return c.api.ApplyNixConfig(ctx, &nodepb.ApplyNixConfigRequest{
		ConfigurationNix: config,
		Rebuild:          rebuild,
	})
}

// parseMemorySize converts memory string (2G, 4096M) to bytes
func parseMemorySize(size string) (int64, error) {
	if len(size) < 2 {
		return 0, fmt.Errorf("invalid memory size: %s", size)
	}

	unit := size[len(size)-1:]
	value := size[:len(size)-1]

	var num int64
	_, err := fmt.Sscanf(value, "%d", &num)
	if err != nil {
		return 0, fmt.Errorf("invalid memory value: %s", value)
	}

	switch unit {
	case "G", "g":
		return num * 1024 * 1024 * 1024, nil
	case "M", "m":
		return num * 1024 * 1024, nil
	case "K", "k":
		return num * 1024, nil
	default:
		return 0, fmt.Errorf("invalid memory unit: %s (use G, M, or K)", unit)
	}
}

// parseDiskSize converts disk string (100G, 50000M) to bytes
func parseDiskSize(size string) (int64, error) {
	return parseMemorySize(size) // Same logic
}

// formatBytes converts bytes to human-readable format
func formatBytes(bytes int64) string {
	const unit = 1024
	if bytes < unit {
		return fmt.Sprintf("%d B", bytes)
	}
	div, exp := int64(unit), 0
	for n := bytes / unit; n >= unit; n /= unit {
		div *= unit
		exp++
	}
	return fmt.Sprintf("%.1f %cB", float64(bytes)/float64(div), "KMGTPE"[exp])
}
