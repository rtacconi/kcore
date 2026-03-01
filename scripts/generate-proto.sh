#!/bin/bash
# generate-proto.sh - Generate protobuf code from .proto files

set -e

# Check if protoc is installed
if ! command -v protoc &> /dev/null; then
    echo "Error: protoc not found. Please install Protocol Buffers compiler."
    echo "On macOS: brew install protobuf"
    echo "On Linux: apt-get install protobuf-compiler"
    exit 1
fi

# Check if go plugins are installed
if ! go list -m google.golang.org/protobuf/cmd/protoc-gen-go > /dev/null 2>&1; then
    echo "Installing protoc-gen-go..."
    go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
fi

if ! go list -m google.golang.org/grpc/cmd/protoc-gen-go-grpc > /dev/null 2>&1; then
    echo "Installing protoc-gen-go-grpc..."
    go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@latest
fi

echo "Generating protobuf code..."

# Create api directories if they don't exist
mkdir -p api/node api/controller

# Generate into proto/ using source_relative paths, then move into api/*

# Node protos
protoc --go_out=. --go_opt=paths=source_relative \
       --go-grpc_out=. --go-grpc_opt=paths=source_relative \
       proto/node.proto

mv proto/node.pb.go api/node/node.pb.go
mv proto/node_grpc.pb.go api/node/node_grpc.pb.go

# Controller protos
protoc --go_out=. --go_opt=paths=source_relative \
       --go-grpc_out=. --go-grpc_opt=paths=source_relative \
       proto/controller.proto

mv proto/controller.pb.go api/controller/controller.pb.go
mv proto/controller_grpc.pb.go api/controller/controller_grpc.pb.go

echo "Protobuf code generated successfully!"
