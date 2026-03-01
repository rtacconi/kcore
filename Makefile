.PHONY: proto tools build clean

# USB vendor safety check (can override: USB_VENDOR=Kingston or USB_VENDOR="" or FORCE=1)
USB_VENDOR ?= SanDisk
SUDO ?= sudo

PROTOC_GEN_GO := $(shell which protoc-gen-go || true)
PROTOC_GEN_GO_GRPC := $(shell which protoc-gen-go-grpc || true)

TOOLS_VERSION_PROTOBUF := v1.34.0
TOOLS_VERSION_GRPC := v1.4.0

proto: tools
	mkdir -p gen/go
	protoc \
		--proto_path=. \
		--go_out=gen/go --go_opt=paths=source_relative \
		--go-grpc_out=gen/go --go-grpc_opt=paths=source_relative \
		api/v1/compute.proto

build:
	mkdir -p bin
	go build -o bin/node-agent ./cmd/node-agent

clean:
	rm -rf bin gen

# Install protoc plugins if missing
tools:
	@if [ -z "$(PROTOC_GEN_GO)" ]; then \
		GO111MODULE=on go install google.golang.org/protobuf/cmd/protoc-gen-go@$(TOOLS_VERSION_PROTOBUF); \
	fi
	@if [ -z "$(PROTOC_GEN_GO_GRPC)" ]; then \
		GO111MODULE=on go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@$(TOOLS_VERSION_GRPC); \
	fi

.PHONY: iso
iso:
	./scripts/build-iso.sh
