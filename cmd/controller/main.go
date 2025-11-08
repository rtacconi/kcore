package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/kcore/kcore/pkg/config"
	"github.com/kcore/kcore/pkg/controller"
	"github.com/kcore/kcore/pkg/sqlite"
)

func main() {
	configPath := flag.String("config", "./controller.yaml", "Path to controller configuration file")
	vmSpecPath := flag.String("apply-vm", "", "Path to VM YAML spec to apply")
	flag.Parse()

	// Load configuration
	var cfg *config.ControllerConfig
	var err error
	if _, err := os.Stat(*configPath); err == nil {
		cfg, err = config.LoadControllerConfig(*configPath)
		if err != nil {
			log.Fatalf("Failed to load config: %v", err)
		}
	} else {
		log.Printf("Config file not found, using defaults")
		cfg = config.DefaultControllerConfig()
	}

	// Initialize database
	db, err := sqlite.New(cfg.DatabasePath)
	if err != nil {
		log.Fatalf("Failed to initialize database: %v", err)
	}
	defer db.Close()

	// Create controller
	ctrl, err := controller.New(db, cfg)
	if err != nil {
		log.Fatalf("Failed to create controller: %v", err)
	}
	defer ctrl.Close()

	// If applying a VM spec, do that and exit
	if *vmSpecPath != "" {
		vmSpec, err := config.ParseVMFromFile(*vmSpecPath)
		if err != nil {
			log.Fatalf("Failed to parse VM spec: %v", err)
		}

		if err := ctrl.ApplyVM(context.Background(), vmSpec); err != nil {
			log.Fatalf("Failed to apply VM: %v", err)
		}

		fmt.Printf("Successfully applied VM spec: %s\n", vmSpec.Metadata.Name)
		return
	}

	// Start reconciliation loop
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Handle signals
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		<-sigChan
		log.Println("Shutting down...")
		cancel()
	}()

	log.Printf("Starting kcore controller on %s", cfg.ListenAddr)
	log.Printf("Database: %s", cfg.DatabasePath)

	// Start gRPC server for node registration
	go func() {
		if err := ctrl.StartServer(ctx); err != nil && err != context.Canceled {
			log.Printf("gRPC server error: %v", err)
		}
	}()

	// Start reconciliation loop
	if err := ctrl.Reconcile(ctx); err != nil && err != context.Canceled {
		log.Fatalf("Reconciliation error: %v", err)
	}

	log.Println("Controller stopped")
}
