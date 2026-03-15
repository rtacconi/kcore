// +build ignore

package main

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"flag"
	"fmt"
	"log"
	"os"
	"time"

	ctrlpb "github.com/kcore/kcore/api/controller"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
)

func main() {
	controller := flag.String("controller", "localhost:9090", "Controller address")
	nodeID := flag.String("node-id", "lenovo-node", "Node ID")
	nodeAddr := flag.String("node-addr", "192.168.40.107:9091", "Node address")
	hostname := flag.String("hostname", "lenovo", "Node hostname")
	certFile := flag.String("cert", "certs/controller.crt", "Client cert")
	keyFile := flag.String("key", "certs/controller.key", "Client key")
	caFile := flag.String("ca", "certs/ca.crt", "CA cert")
	flag.Parse()

	cert, err := tls.LoadX509KeyPair(*certFile, *keyFile)
	if err != nil {
		log.Fatalf("load certs: %v", err)
	}

	caCert, err := os.ReadFile(*caFile)
	if err != nil {
		log.Fatalf("read CA: %v", err)
	}
	pool := x509.NewCertPool()
	pool.AppendCertsFromPEM(caCert)

	creds := credentials.NewTLS(&tls.Config{
		Certificates:       []tls.Certificate{cert},
		RootCAs:            pool,
		InsecureSkipVerify: true,
		MinVersion:         tls.VersionTLS12,
	})

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	conn, err := grpc.DialContext(ctx, *controller, grpc.WithTransportCredentials(creds))
	if err != nil {
		log.Fatalf("dial: %v", err)
	}
	defer conn.Close()

	client := ctrlpb.NewControllerClient(conn)
	resp, err := client.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId:   *nodeID,
		Hostname: *hostname,
		Address:  *nodeAddr,
		Capacity: &ctrlpb.NodeCapacity{
			CpuCores:    8,
			MemoryBytes: 32 * 1024 * 1024 * 1024,
		},
	})
	if err != nil {
		log.Fatalf("register: %v", err)
	}
	fmt.Printf("Node registered: success=%v message=%q\n", resp.Success, resp.Message)
}
