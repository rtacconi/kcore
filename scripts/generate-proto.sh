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

# Create api directory if it doesn't exist
mkdir -p api/node api/controller

echo "Generating protobuf code..."

# Generate node.proto
protoc --go_out=. --go_opt=paths=source_relative \
    --go-grpc_out=. --go-grpc_opt=paths=source_relative \
    proto/node.proto

# Generate controller.proto
protoc --go_out=. --go_opt=paths=source_relative \
    --go-grpc_out=. --go-grpc_opt=paths=source_relative \
    proto/controller.proto

echo "Protobuf code generated successfully!"

