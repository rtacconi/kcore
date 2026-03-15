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
	nodepb "github.com/kcore/kcore/api/node"
)

// ---------------------------------------------------------------------------
// ControllerVMClient – all VM operations go through the controller
// ---------------------------------------------------------------------------

type ControllerVMClient struct {
	conn *grpc.ClientConn
	api  ctrlpb.ControllerClient
}

func NewControllerVMClient(address string, tlsInsecure bool, certFile, keyFile, caFile string) (*ControllerVMClient, error) {
	opts, err := buildGRPCDialOpts(tlsInsecure, certFile, keyFile, caFile)
	if err != nil {
		return nil, err
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	conn, err := grpc.DialContext(ctx, address, opts...)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to controller %s: %w", address, err)
	}

	return &ControllerVMClient{
		conn: conn,
		api:  ctrlpb.NewControllerClient(conn),
	}, nil
}

func (c *ControllerVMClient) Close() error {
	if c.conn != nil {
		return c.conn.Close()
	}
	return nil
}

func (c *ControllerVMClient) CreateVM(ctx context.Context, spec *ctrlpb.VmSpec, imageURI, cloudInit string) (*ctrlpb.CreateVmResponse, error) {
	return c.api.CreateVm(ctx, &ctrlpb.CreateVmRequest{
		Spec:              spec,
		ImageUri:          imageURI,
		CloudInitUserData: cloudInit,
	})
}

func (c *ControllerVMClient) DeleteVM(ctx context.Context, vmID string) error {
	_, err := c.api.DeleteVm(ctx, &ctrlpb.DeleteVmRequest{VmId: vmID})
	return err
}

func (c *ControllerVMClient) StartVM(ctx context.Context, vmID string) error {
	_, err := c.api.StartVm(ctx, &ctrlpb.StartVmRequest{VmId: vmID})
	return err
}

func (c *ControllerVMClient) StopVM(ctx context.Context, vmID string, force bool) error {
	_, err := c.api.StopVm(ctx, &ctrlpb.StopVmRequest{VmId: vmID, Force: force})
	return err
}

func (c *ControllerVMClient) GetVM(ctx context.Context, vmID string) (*ctrlpb.GetVmResponse, error) {
	return c.api.GetVm(ctx, &ctrlpb.GetVmRequest{VmId: vmID})
}

func (c *ControllerVMClient) ListVMs(ctx context.Context) ([]*ctrlpb.VmInfo, error) {
	resp, err := c.api.ListVms(ctx, &ctrlpb.ListVmsRequest{})
	if err != nil {
		return nil, err
	}
	return resp.Vms, nil
}

func (c *ControllerVMClient) ListNodes(ctx context.Context) ([]*ctrlpb.NodeInfo, error) {
	resp, err := c.api.ListNodes(ctx, &ctrlpb.ListNodesRequest{})
	if err != nil {
		return nil, err
	}
	return resp.Nodes, nil
}

func (c *ControllerVMClient) GetNode(ctx context.Context, nodeID string) (*ctrlpb.NodeInfo, error) {
	resp, err := c.api.GetNode(ctx, &ctrlpb.GetNodeRequest{NodeId: nodeID})
	if err != nil {
		return nil, err
	}
	return resp.Node, nil
}

// ---------------------------------------------------------------------------
// NodeClient – kept for node-specific admin ops (images, nix config)
// ---------------------------------------------------------------------------

type NodeClient struct {
	conn    *grpc.ClientConn
	compute nodepb.NodeComputeClient
	storage nodepb.NodeStorageClient
	info    nodepb.NodeInfoClient
}

func NewNodeClient(address string, tlsInsecure bool, certFile, keyFile, caFile string) (*NodeClient, error) {
	opts, err := buildGRPCDialOpts(tlsInsecure, certFile, keyFile, caFile)
	if err != nil {
		return nil, err
	}

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

func (c *NodeClient) Close() error {
	if c.conn != nil {
		return c.conn.Close()
	}
	return nil
}

func (c *NodeClient) GetNodeInfo(ctx context.Context) (*nodepb.GetNodeInfoResponse, error) {
	return c.info.GetNodeInfo(ctx, &nodepb.GetNodeInfoRequest{})
}

// ---------------------------------------------------------------------------
// ControllerAdminClient
// ---------------------------------------------------------------------------

type ControllerAdminClient struct {
	conn *grpc.ClientConn
	api  ctrlpb.ControllerAdminClient
}

func NewControllerAdminClient(address string, tlsInsecure bool, certFile, keyFile, caFile string) (*ControllerAdminClient, error) {
	opts, err := buildGRPCDialOpts(tlsInsecure, certFile, keyFile, caFile)
	if err != nil {
		return nil, err
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

// ---------------------------------------------------------------------------
// NodeAdminClient
// ---------------------------------------------------------------------------

type NodeAdminClient struct {
	conn *grpc.ClientConn
	api  nodepb.NodeAdminClient
}

func NewNodeAdminClient(address string, tlsInsecure bool, certFile, keyFile, caFile string) (*NodeAdminClient, error) {
	opts, err := buildGRPCDialOpts(tlsInsecure, certFile, keyFile, caFile)
	if err != nil {
		return nil, err
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

func (c *NodeAdminClient) Close() error {
	if c.conn != nil {
		return c.conn.Close()
	}
	return nil
}

func (c *NodeAdminClient) ApplyNixConfig(ctx context.Context, config string, rebuild bool) (*nodepb.ApplyNixConfigResponse, error) {
	return c.api.ApplyNixConfig(ctx, &nodepb.ApplyNixConfigRequest{
		ConfigurationNix: config,
		Rebuild:          rebuild,
	})
}

// ---------------------------------------------------------------------------
// Shared TLS helpers
// ---------------------------------------------------------------------------

func buildGRPCDialOpts(tlsInsecure bool, certFile, keyFile, caFile string) ([]grpc.DialOption, error) {
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

	return opts, nil
}

// ---------------------------------------------------------------------------
// Size helpers
// ---------------------------------------------------------------------------

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

func parseDiskSize(size string) (int64, error) {
	return parseMemorySize(size)
}

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
