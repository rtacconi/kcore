.PHONY: all build controller node-agent proto test clean deploy

all: proto build

# Generate protobuf code
proto:
	@echo "Generating protobuf code..."
	@mkdir -p api
	@protoc --go_out=. --go_opt=paths=source_relative \
		--go-grpc_out=. --go-grpc_opt=paths=source_relative \
		proto/*.proto

# Build control plane (macOS)
controller:
	@echo "Building control plane..."
	@go build -o bin/kcore-controller ./cmd/controller

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

