package main

import (
	"flag"
	"log"
	"net"
	"os"
	"os/signal"
	"syscall"

	"google.golang.org/grpc"

	ctrlpb "github.com/kcore/kcore/api/controller"
	"github.com/kcore/kcore/pkg/controller"
)

func main() {
	listenAddr := flag.String("listen", ":8080", "Address to listen on (default :8080)")
	flag.Parse()

	// Create controller server
	server := controller.NewServer()

	// Create gRPC server
	grpcServer := grpc.NewServer()
	ctrlpb.RegisterControllerServer(grpcServer, server)

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
