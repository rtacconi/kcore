.PHONY: all build controller kctl install-kctl-local install-kctl-system node-agent proto test clean deploy
.PHONY: build-iso create-vm delete-vm test-node list-services deploy-node deploy-node-agent write-usb help

all: proto build

# Generate protobuf code
proto:
	@echo "Generating protobuf code..."
	@mkdir -p api
	@protoc --go_out=. --go_opt=paths=source_relative \
		--go-grpc_out=. --go-grpc_opt=paths=source_relative \
		proto/*.proto

# Build controller (macOS)
controller:
	@echo "Building controller..."
	@go build -o bin/kcore-controller ./cmd/controller

# Build kctl CLI (macOS ARM64)
kctl:
	@echo "Building kctl for macOS ARM64..."
	@GOOS=darwin GOARCH=arm64 go build -o bin/kctl ./cmd/kctl
	@echo "✅ kctl built successfully: bin/kctl"
	@echo ""
	@echo "To use kctl:"
	@echo "  ./bin/kctl [command]              # Run directly"
	@echo "  make install-kctl-local           # Install to ~/.local/bin"
	@echo "  make install-kctl-system          # Install to /usr/local/bin"

# Install kctl to user's local bin
install-kctl-local: kctl
	@./scripts/install-kctl.sh --local

# Install kctl to system bin (requires sudo)
install-kctl-system: kctl
	@./scripts/install-kctl.sh --system

# Build node agent (Linux/amd64) - requires CGO, use Podman for cross-compilation
# NOTE: Podman on macOS has emulation issues. For best results, build on Linux or use Nix.
node-agent:
	@echo "Cross-compiling node agent for Linux/amd64..."
	@echo "WARNING: Podman emulation on macOS may fail. Consider building on Linux instead."
	@echo "See BUILD_NODE_AGENT.md for alternatives."
	@podman run --rm --platform linux/amd64 \
		-v $(PWD):/work \
		-w /work \
		-e CGO_ENABLED=1 \
		-e GOOS=linux \
		-e GOARCH=amd64 \
		docker.io/golang:1.24 bash -c 'apt-get update -qq && apt-get install -y -qq libvirt-dev pkg-config gcc && go build -o bin/kcore-node-agent-linux-amd64 ./cmd/node-agent' || \
		(echo ""; echo "Build failed. This is expected on macOS with Podman."; echo "Try: make node-agent-nix OR build on a Linux system"; exit 1)

# Build node agent using Nix (works on macOS)
node-agent-nix:
	@echo "Building node-agent using Nix..."
	@nix --extra-experimental-features nix-command --extra-experimental-features flakes build .#packages.x86_64-linux.node-agent -o result-node-agent
	@mkdir -p bin
	@cp result-node-agent/bin/kcore-node-agent bin/kcore-node-agent-linux-amd64
	@rm -rf result-node-agent
	@echo "Built: bin/kcore-node-agent-linux-amd64"

# Build node agent in Podman (internal target)
node-agent-podman:
	@GOOS=linux GOARCH=amd64 go build -o bin/kcore-node-agent-linux-amd64 ./cmd/node-agent

# Build both
build: controller node-agent

# Run tests
test:
	@go test ./...

# Run integration tests
test-integration:
	@./scripts/run-integration-tests.sh

# Run specific integration test suites
test-controller:
	@go test ./test/integration/controller/... -v

test-e2e:
	@./test/integration/e2e/full_workflow_test.sh

test-multi-node:
	@./test/integration/e2e/multi_node_test.sh

test-kctl:
	@./test/integration/kctl/kctl_test.sh

# Run all tests (unit + integration)
test-all: test test-integration

# Clean build artifacts
clean:
	@rm -rf bin/ api/

# Deploy node agent to a ThinkCentre node
# Usage: make deploy NODE=192.168.1.100 USER=root
deploy:
	@if [ -z "$(NODE)" ]; then \
		echo "Error: NODE variable required. Usage: make deploy NODE=192.168.1.100 USER=root"; \
		exit 1; \
	fi
	@USER=$${USER:-root}; \
	echo "Deploying to $$NODE as $$USER..."; \
	scp bin/kcore-node-agent-linux-amd64 $$USER@$$NODE:/tmp/kcore-node-agent; \
	ssh $$USER@$$NODE 'sudo mv /tmp/kcore-node-agent /opt/kcode/kcore-node-agent && sudo chmod +x /opt/kcode/kcore-node-agent && sudo systemctl restart kcode-node-agent'

#
# ISO and Node Management
#

# Build bootable NixOS ISO with kcore embedded
# Usage: make build-iso
build-iso:
	@./scripts/build-iso.sh

# Create a test VM on node
# Usage: NODE_IP=192.168.40.146 make create-vm
create-vm:
	@./scripts/create-vm.sh

# Delete a VM from node
# Usage: NODE_IP=192.168.40.146 VM_ID=<uuid> make delete-vm
delete-vm:
	@./scripts/delete-vm.sh

# Test node connectivity and gRPC service
# Usage: NODE_IP=192.168.40.146 make test-node
test-node:
	@./scripts/test-node.sh

# List available gRPC services on node
# Usage: NODE_IP=192.168.40.146 make list-services
list-services:
	@./scripts/list-services.sh

# Deploy node-agent binary and config to node
# Usage: NODE_IP=192.168.40.146 make deploy-node
deploy-node:
	@./scripts/deploy-node.sh

# Deploy node agent binary to running node
# Usage: make deploy-node-agent NODE_HOST=root@192.168.40.146
deploy-node-agent:
	@./scripts/deploy-node-agent.sh $(NODE_HOST)

# Write ISO to USB drive
# Usage: USB_DEVICE=/dev/disk4 make write-usb (macOS)
# Usage: USB_DEVICE=/dev/sdb make write-usb (Linux)
write-usb:
	@./scripts/write-usb.sh

# Show help with all available commands
help:
	@echo "🔧 KCORE Makefile Targets"
	@echo ""
	@echo "📦 Building:"
	@echo "  make proto                - Generate protobuf code"
	@echo "  make controller           - Build controller (macOS/Linux)"
	@echo "  make kctl                 - Build kctl CLI (macOS ARM64)"
	@echo "  make install-kctl-local   - Install kctl to ~/.local/bin"
	@echo "  make install-kctl-system  - Install kctl to /usr/local/bin (sudo)"
	@echo "  make node-agent           - Build node-agent (Podman)"
	@echo "  make node-agent-nix       - Build node-agent (Nix)"
	@echo "  make build-iso            - Build bootable kcore ISO"
	@echo "  make build                - Build controller + node-agent"
	@echo ""
	@echo "🧪 Testing:"
	@echo "  make test               - Run Go unit tests"
	@echo "  make test-integration   - Run all integration tests"
	@echo "  make test-controller    - Run controller integration tests"
	@echo "  make test-e2e           - Run end-to-end tests"
	@echo "  make test-multi-node    - Run multi-node tests"
	@echo "  make test-kctl          - Run kctl tests"
	@echo "  make test-all           - Run all tests (unit + integration)"
	@echo ""
	@echo "☁️  Node Management (requires NODE_IP=<ip>):"
	@echo "  make test-node          - Test node connectivity"
	@echo "  make list-services      - List gRPC services on node"
	@echo "  make deploy-node        - Deploy node-agent to node"
	@echo "  make create-vm          - Create test VM on node"
	@echo "  make delete-vm          - Delete VM (requires VM_ID=<uuid>)"
	@echo ""
	@echo "💾 Installation:"
	@echo "  make write-usb          - Write ISO to USB (requires USB_DEVICE=/dev/diskX)"
	@echo ""
	@echo "🗑️  Cleanup:"
	@echo "  make clean              - Remove build artifacts"
	@echo ""
	@echo "Examples:"
	@echo "  make build-iso"
	@echo "  NODE_IP=192.168.40.146 make create-vm"
	@echo "  NODE_IP=192.168.40.146 VM_ID=<uuid> make delete-vm"
	@echo "  USB_DEVICE=/dev/disk4 make write-usb"
	@echo ""
	@echo "📚 Documentation:"
	@echo "  docs/QUICKSTART.md    - Installation guide"
	@echo "  docs/ARCHITECTURE.md  - System design"
	@echo "  docs/COMMANDS.md      - Command reference"
	@echo "  docs/FIXES.md         - Troubleshooting"

