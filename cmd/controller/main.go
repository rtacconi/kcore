package main

import (
	"crypto/tls"
	"crypto/x509"
	"flag"
	"log"
	"net"
	"os"
	"os/signal"
	"syscall"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"

	ctrlpb "github.com/kcore/kcore/api/controller"
	"github.com/kcore/kcore/pkg/controller"
)

func main() {
	listenAddr := flag.String("listen", ":8080", "Address to listen on (default :8080)")
	certFile := flag.String("cert", "certs/controller.crt", "TLS certificate file")
	keyFile := flag.String("key", "certs/controller.key", "TLS key file")
	caFile := flag.String("ca", "certs/ca.crt", "CA certificate file")
	flag.Parse()

	// Load TLS credentials
	cert, err := tls.LoadX509KeyPair(*certFile, *keyFile)
	if err != nil {
		log.Fatalf("Failed to load server certificate: %v", err)
	}

	caCert, err := os.ReadFile(*caFile)
	if err != nil {
		log.Fatalf("Failed to read CA cert: %v", err)
	}

	certPool := x509.NewCertPool()
	if !certPool.AppendCertsFromPEM(caCert) {
		log.Fatalf("Failed to append CA cert")
	}

	tlsConfig := &tls.Config{
		Certificates: []tls.Certificate{cert},
		ClientAuth:   tls.RequireAndVerifyClientCert,
		ClientCAs:    certPool,
		MinVersion:   tls.VersionTLS12,
	}

	// Create controller server
	server := controller.NewServer()

	// Create gRPC server with TLS
	grpcServer := grpc.NewServer(grpc.Creds(credentials.NewTLS(tlsConfig)))
	ctrlpb.RegisterControllerServer(grpcServer, server)
	ctrlpb.RegisterControllerAdminServer(grpcServer, server)

	// Start listening
	listener, err := net.Listen("tcp", *listenAddr)
	if err != nil {
		log.Fatalf("Failed to listen: %v", err)
	}

	// Handle graceful shutdown
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		<-sigChan
		log.Println("Shutting down controller...")
		grpcServer.GracefulStop()
	}()

	log.Printf("🚀 kcore Controller starting on %s", *listenAddr)
	log.Printf("   Waiting for nodes to register...")
	log.Printf("   Ready to accept VM operations from kctl")

	if err := grpcServer.Serve(listener); err != nil {
		log.Fatalf("Failed to serve: %v", err)
	}

	log.Println("Controller stopped")
}
