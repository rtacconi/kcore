.PHONY: tf-build tf-install tf-test tf-clean tf-fmt tf-lint

# Binary name
BINARY=terraform-provider-kcore

# Build settings
BUILD_DIR=.
GO=go
GOFLAGS=-v

# Detect OS and ARCH
GOOS=$(shell go env GOOS)
GOARCH=$(shell go env GOARCH)

# Terraform plugin directory
PLUGIN_DIR=$(HOME)/.terraform.d/plugins/registry.terraform.io/kcore/kcore/0.1.0/$(GOOS)_$(GOARCH)

all: tf-build

tf-build:
	@echo "Building $(BINARY)..."
	$(GO) build $(GOFLAGS) -o $(BUILD_DIR)/$(BINARY)

tf-install: tf-build
	@echo "Installing provider to $(PLUGIN_DIR)..."
	@mkdir -p $(PLUGIN_DIR)
	@cp $(BUILD_DIR)/$(BINARY) $(PLUGIN_DIR)/$(BINARY)
	@echo "Provider installed successfully!"
	@echo "You can now use it in your Terraform configurations"

tf-test:
	@echo "Running tests..."
	$(GO) test ./... -v

tf-test-acc:
	@echo "Running acceptance tests..."
	TF_ACC=1 $(GO) test ./... -v -timeout 120m

tf-fmt:
	@echo "Formatting code..."
	$(GO) fmt ./...

tf-lint:
	@echo "Running linter..."
	golangci-lint run

tf-clean:
	@echo "Cleaning..."
	@rm -f $(BINARY)
	@rm -rf dist/

tf-deps:
	@echo "Downloading dependencies..."
	$(GO) mod download
	$(GO) mod tidy

.PHONY: help
help:
	@echo "Available targets:"
	@echo "  tf-build      - Build the provider binary"
	@echo "  tf-install    - Build and install the provider locally"
	@echo "  tf-test       - Run unit tests"
	@echo "  tf-test-acc   - Run acceptance tests"
	@echo "  tf-fmt        - Format code"
	@echo "  tf-lint       - Run linter"
	@echo "  tf-clean      - Clean build artifacts"
	@echo "  tf-deps       - Download and tidy dependencies"

