package main

import (
	"flag"
	"log"
	"net"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"

	pb "kcore/gen/api/v1"
	"kcore/internal/agent"
)

func main() {
	listen := flag.String("listen", ":8443", "address to listen on")
	dataDir := flag.String("data-dir", "/var/lib/kcore", "data directory for VM files")
	nodeName := flag.String("node", "", "node name to report")
	certFile := flag.String("tls-cert", "", "server certificate (PEM)")
	keyFile := flag.String("tls-key", "", "server key (PEM)")
	caFile := flag.String("tls-ca", "", "CA certificate for client auth (PEM)")
	flag.Parse()

	ag, err := agent.NewAgent(*dataDir, *nodeName)
	if err != nil {
		log.Fatalf("init agent: %v", err)
	}
	defer ag.Close()

	lis, err := net.Listen("tcp", *listen)
	if err != nil {
		log.Fatalf("listen: %v", err)
	}

	var opts []grpc.ServerOption
	if *certFile != "" && *keyFile != "" {
		creds, err := credentials.NewServerTLSFromFile(*certFile, *keyFile)
		if err != nil {
			log.Fatalf("tls: %v", err)
		}
		opts = append(opts, grpc.Creds(creds))
	}

	s := grpc.NewServer(opts...)
	pb.RegisterNodeAgentServer(s, ag)
	log.Printf("node-agent listening on %s", *listen)
	log.Fatal(s.Serve(lis))
}
