package controller

import (
	"context"
	"log"
	"os"
	"time"

	ctrlpb "github.com/kcore/kcore/api/controller"
	nodepb "github.com/kcore/kcore/api/node"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

// AutoRegisterLocalNode registers localhost:9091 as a node in bootstrap mode.
// It retries until the node-agent is reachable, with exponential backoff.
func AutoRegisterLocalNode(server *Server, nodeInsecure bool) {
	const localAddr = "localhost:9091"
	hostname, _ := os.Hostname()
	if hostname == "" {
		hostname = "bootstrap-node"
	}

	backoff := time.Second
	for {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)

		var dialOpts []grpc.DialOption
		if nodeInsecure {
			dialOpts = append(dialOpts, grpc.WithTransportCredentials(insecure.NewCredentials()))
		}

		conn, err := grpc.DialContext(ctx, localAddr, dialOpts...)
		if err != nil {
			cancel()
			log.Printf("Auto-register: cannot dial %s: %v (retrying in %s)", localAddr, err, backoff)
			time.Sleep(backoff)
			if backoff < 30*time.Second {
				backoff *= 2
			}
			continue
		}

		infoClient := nodepb.NewNodeInfoClient(conn)
		infoResp, err := infoClient.GetNodeInfo(ctx, &nodepb.GetNodeInfoRequest{})
		cancel()

		var capacity *ctrlpb.NodeCapacity
		var nodeID string
		if err != nil {
			log.Printf("Auto-register: GetNodeInfo failed (using defaults): %v", err)
			nodeID = "bootstrap-node"
			capacity = &ctrlpb.NodeCapacity{CpuCores: 1, MemoryBytes: 1024 * 1024 * 1024}
		} else {
			nodeID = infoResp.NodeId
			if nodeID == "" {
				nodeID = hostname
			}
			capacity = &ctrlpb.NodeCapacity{
				CpuCores:    infoResp.Capacity.GetCpuCores(),
				MemoryBytes: infoResp.Capacity.GetMemoryBytes(),
			}
		}

		conn.Close()

		_, regErr := server.RegisterNode(context.Background(), &ctrlpb.RegisterNodeRequest{
			NodeId:   nodeID,
			Hostname: hostname,
			Address:  localAddr,
			Capacity: capacity,
		})
		if regErr != nil {
			log.Printf("Auto-register: RegisterNode failed: %v", regErr)
		} else {
			log.Printf("Auto-register: node %s registered at %s", nodeID, localAddr)
		}
		return
	}
}
