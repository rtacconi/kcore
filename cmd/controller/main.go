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
	"google.golang.org/grpc/credentials/insecure"

	ctrlpb "github.com/kcore/kcore/api/controller"
	cppb "github.com/kcore/kcore/api/controlplane"
	"github.com/kcore/kcore/pkg/controller"
	"github.com/kcore/kcore/pkg/controlplane"
	"github.com/kcore/kcore/pkg/sqlite"
)

func main() {
	listenAddr := flag.String("listen", ":8080", "Address to listen on (default :8080)")
	certFile := flag.String("cert", "certs/controller.crt", "TLS certificate file")
	keyFile := flag.String("key", "certs/controller.key", "TLS key file")
	caFile := flag.String("ca", "certs/ca.crt", "CA certificate file")
	dbPath := flag.String("db", "./kcore-controller.db", "SQLite database path")
	insecureMode := flag.Bool("insecure", false, "Bootstrap mode: plain gRPC without TLS")
	nodeInsecure := flag.Bool("node-insecure", false, "Dial node-agents without TLS")
	autoRegisterLocal := flag.Bool("auto-register-local", false, "Auto-register localhost:9091 as a node")
	flag.Parse()

	db, err := sqlite.New(*dbPath)
	if err != nil {
		log.Fatalf("Failed to open database at %s: %v", *dbPath, err)
	}
	defer db.Close()
	log.Printf("Database opened: %s (schema version %d)", *dbPath, db.SchemaVersion())

	server := controller.NewServerWithDB(db)
	controlPlaneServer := controlplane.NewService(server)

	var grpcServer *grpc.Server

	if *insecureMode {
		log.Println("BOOTSTRAP MODE: running without TLS (insecure)")
		grpcServer = grpc.NewServer()

		if *nodeInsecure {
			server.SetNodeDialCredentials(insecure.NewCredentials())
		}
	} else {
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

		server.SetNodeDialCredentials(credentials.NewTLS(&tls.Config{
			Certificates:       []tls.Certificate{cert},
			RootCAs:            certPool,
			InsecureSkipVerify: true,
			MinVersion:         tls.VersionTLS12,
		}))

		grpcServer = grpc.NewServer(grpc.Creds(credentials.NewTLS(tlsConfig)))
	}

	ctrlpb.RegisterControllerServer(grpcServer, server)
	ctrlpb.RegisterControllerAdminServer(grpcServer, server)
	cppb.RegisterControlPlaneServer(grpcServer, controlPlaneServer)

	listener, err := net.Listen("tcp", *listenAddr)
	if err != nil {
		log.Fatalf("Failed to listen: %v", err)
	}

	if *autoRegisterLocal {
		go func() {
			log.Println("Auto-registering localhost:9091 as bootstrap node...")
			controller.AutoRegisterLocalNode(server, *nodeInsecure)
		}()
	}

	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		<-sigChan
		log.Println("Shutting down controller...")
		grpcServer.GracefulStop()
	}()

	log.Printf("kcore Controller starting on %s", *listenAddr)
	if *insecureMode {
		log.Printf("   Mode: BOOTSTRAP (no TLS)")
	} else {
		log.Printf("   Mode: Production (mTLS)")
	}
	log.Printf("   Database: %s", *dbPath)
	log.Printf("   Ready to accept operations from kctl and Terraform")

	if err := grpcServer.Serve(listener); err != nil {
		log.Fatalf("Failed to serve: %v", err)
	}

	log.Println("Controller stopped")
}
