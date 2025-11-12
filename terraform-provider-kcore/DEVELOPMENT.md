# Development Guide for KCore Terraform Provider

This guide covers development, testing, and contributing to the KCore Terraform provider.

## Prerequisites

- Go 1.24 or later
- Terraform 1.0 or later
- Access to a running KCore controller
- Make (optional, but recommended)

## Project Structure

```
terraform-provider-kcore/
├── internal/
│   └── provider/
│       ├── provider.go              # Main provider implementation
│       ├── provider_test.go         # Provider tests
│       ├── resource_vm.go           # VM resource implementation
│       ├── resource_vm_test.go      # VM resource tests
│       ├── data_source_vm.go        # VM data source
│       └── data_source_node.go      # Node data sources
├── examples/
│   ├── main.tf                      # Basic example
│   └── complete/                    # Complete example with multiple resources
├── main.go                          # Provider entry point
├── go.mod                           # Go module definition
├── Makefile                         # Build automation
└── README.md                        # User documentation
```

## Building the Provider

### Using Make

```bash
# Build the provider
make tf-build

# Build and install locally for testing
make tf-install

# Run tests
make tf-test

# Run acceptance tests
make tf-test-acc

# Format code
make tf-fmt
```

### Manual Build

```bash
# Download dependencies
go mod download
go mod tidy

# Build the provider
go build -o terraform-provider-kcore

# Install locally
mkdir -p ~/.terraform.d/plugins/registry.terraform.io/kcore/kcore/0.1.0/$(go env GOOS)_$(go env GOARCH)/
cp terraform-provider-kcore ~/.terraform.d/plugins/registry.terraform.io/kcore/kcore/0.1.0/$(go env GOOS)_$(go env GOARCH)/
```

## Testing

### Unit Tests

Unit tests verify individual functions and logic without requiring external dependencies.

```bash
# Run unit tests
go test ./... -v

# Run tests with coverage
go test ./... -v -cover -coverprofile=coverage.out

# View coverage report
go tool cover -html=coverage.out
```

### Acceptance Tests

Acceptance tests verify the provider works correctly with a real KCore controller.

**Prerequisites:**
- Running KCore controller
- Set `KCORE_CONTROLLER_ADDRESS` environment variable
- Set `TF_ACC=1` to enable acceptance tests

```bash
# Export required environment variables
export KCORE_CONTROLLER_ADDRESS="localhost:9090"
export TF_ACC=1

# Run acceptance tests
go test ./... -v -timeout 120m

# Or use make
make test-acc
```

### Manual Testing

1. Build and install the provider:
```bash
make tf-install
```

2. Create a test directory with Terraform configuration:
```bash
mkdir -p /tmp/tf-test
cd /tmp/tf-test
```

3. Create a `main.tf` file:
```hcl
terraform {
  required_providers {
    kcore = {
      source = "kcore/kcore"
    }
  }
}

provider "kcore" {
  controller_address = "localhost:9090"
  insecure           = true
}

resource "kcore_vm" "test" {
  name         = "test-vm"
  cpu          = 2
  memory_bytes = 4294967296

  disk {
    name           = "root"
    backend_handle = "/tmp/test-root.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  nic {
    network = "default"
    model   = "virtio"
  }
}
```

4. Initialize and apply:
```bash
terraform init
terraform plan
terraform apply
```

## Debugging

### Enable Terraform Debug Logging

```bash
export TF_LOG=DEBUG
export TF_LOG_PATH=./terraform.log
terraform apply
```

### Debug the Provider with Delve

1. Build the provider with debug flags:
```bash
go build -gcflags="all=-N -l" -o terraform-provider-kcore
```

2. Run with debugger support:
```bash
dlv exec ./terraform-provider-kcore -- -debug
```

3. The provider will output a `TF_REATTACH_PROVIDERS` environment variable. Use it:
```bash
export TF_REATTACH_PROVIDERS='...'
terraform apply
```

## Adding New Resources

To add a new resource:

1. Create a new file in `internal/provider/resource_<name>.go`
2. Implement the resource schema and CRUD operations
3. Register the resource in `provider.go`:
```go
ResourcesMap: map[string]*schema.Resource{
    "kcore_vm":      resourceVM(),
    "kcore_newtype": resourceNewType(), // Add your resource
},
```
4. Add tests in `internal/provider/resource_<name>_test.go`
5. Update documentation in `README.md`

## Adding New Data Sources

To add a new data source:

1. Create a new file in `internal/provider/data_source_<name>.go`
2. Implement the data source schema and read operation
3. Register the data source in `provider.go`:
```go
DataSourcesMap: map[string]*schema.Resource{
    "kcore_vm":      dataSourceVM(),
    "kcore_newtype": dataSourceNewType(), // Add your data source
},
```
4. Add tests
5. Update documentation

## Code Style

### Go Code

Follow standard Go conventions:
- Use `gofmt` for formatting
- Follow [Effective Go](https://golang.org/doc/effective_go.html)
- Use meaningful variable names
- Add comments for exported functions

### Terraform Schema

- Use descriptive field names (snake_case)
- Provide clear descriptions for all schema fields
- Set appropriate validation rules
- Mark computed fields correctly
- Use `ForceNew` for fields that require resource recreation

## Common Development Tasks

### Update gRPC Definitions

If the KCore API changes:

1. Regenerate the protobuf files in the main kcore project:
```bash
cd /path/to/kcore
make proto
```

2. Update dependencies in the provider:
```bash
cd terraform-provider-kcore
go get github.com/kcore/kcore@latest
make tf-deps
```

3. Update provider code to match API changes
4. Update tests and documentation

### Release Process

1. Update version in relevant files
2. Update CHANGELOG.md
3. Create a git tag:
```bash
git tag -a v0.2.0 -m "Release v0.2.0"
git push origin v0.2.0
```

4. Build binaries for all platforms:
```bash
GOOS=linux GOARCH=amd64 go build -o terraform-provider-kcore_linux_amd64
GOOS=darwin GOARCH=amd64 go build -o terraform-provider-kcore_darwin_amd64
GOOS=darwin GOARCH=arm64 go build -o terraform-provider-kcore_darwin_arm64
GOOS=windows GOARCH=amd64 go build -o terraform-provider-kcore_windows_amd64.exe
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Ensure tests pass: `make tf-test`
6. Format code: `make tf-fmt`
7. Commit with clear messages
8. Submit a pull request

## Troubleshooting

### Provider Not Found

If Terraform can't find the provider:
- Ensure you've run `make tf-install`
- Check the plugin directory path matches your OS/arch
- Try `terraform init -upgrade`

### Connection Errors

If the provider can't connect to the controller:
- Verify the controller is running
- Check the controller address is correct
- Verify TLS configuration (or use `insecure = true` for testing)
- Check firewall rules

### Schema Validation Errors

If Terraform reports schema validation errors:
- Ensure your provider code matches the Terraform Plugin SDK requirements
- Run `terraform providers schema` to inspect the schema
- Check for typos in field names

## Resources

- [Terraform Plugin SDK Documentation](https://developer.hashicorp.com/terraform/plugin/sdkv2)
- [Terraform Plugin Development Best Practices](https://developer.hashicorp.com/terraform/plugin/best-practices)
- [Go gRPC Documentation](https://grpc.io/docs/languages/go/)
- [KCore Project Documentation](../README.md)

