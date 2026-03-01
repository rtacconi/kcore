# Terraform Provider for KCore

This is a Terraform provider for managing KCore VMs and resources.

## Requirements

- [Terraform](https://www.terraform.io/downloads.html) >= 1.0
- [Go](https://golang.org/doc/install) >= 1.24 (for building the provider)
- KCore controller running and accessible

## Building the Provider

```bash
cd terraform-provider-kcore
go mod tidy
go build -o terraform-provider-kcore
```

## Installing the Provider Locally

For local development, you can install the provider using the following steps:

1. Build the provider:
```bash
make tf-build
```

2. Create a local provider directory:
```bash
mkdir -p ~/.terraform.d/plugins/registry.terraform.io/kcore/kcore/0.1.0/darwin_arm64/
cp terraform-provider-kcore ~/.terraform.d/plugins/registry.terraform.io/kcore/kcore/0.1.0/darwin_arm64/
```

Note: Adjust the OS/arch path based on your system (e.g., `linux_amd64` for Linux).

## Using the Provider

### Provider Configuration

```hcl
terraform {
  required_providers {
    kcore = {
      source = "kcore/kcore"
      version = "~> 0.1.0"
    }
  }
}

provider "kcore" {
  controller_address = "localhost:9090"
  
  # For TLS:
  # tls_cert_path = "/path/to/client.crt"
  # tls_key_path  = "/path/to/client.key"
  # tls_ca_path   = "/path/to/ca.crt"
  
  # For insecure (development only):
  insecure = true
}
```

### Environment Variables

You can also configure the provider using environment variables:

- `KCORE_CONTROLLER_ADDRESS` - Controller gRPC endpoint
- `KCORE_TLS_CERT` - Path to TLS certificate
- `KCORE_TLS_KEY` - Path to TLS key
- `KCORE_TLS_CA` - Path to CA certificate
- `KCORE_INSECURE` - Disable TLS verification (true/false)

### Resources

#### kcore_vm

Manages a KCore VM.

```hcl
resource "kcore_vm" "example" {
  name         = "my-vm"
  cpu          = 2
  memory_bytes = 4294967296 # 4GB

  disk {
    name           = "root"
    backend_handle = "/var/lib/libvirt/images/vm-root.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  nic {
    network = "default"
    model   = "virtio"
  }

  # Optional: specify target node
  target_node = "node-1"
}
```

**Arguments:**

- `name` (Required, String) - Name of the VM
- `cpu` (Required, Int) - Number of CPU cores
- `memory_bytes` (Required, Int) - Memory in bytes
- `target_node` (Optional, String) - Target node to create the VM on
- `disk` (Optional, List) - Disk configuration
  - `name` (Required, String) - Disk name
  - `backend_handle` (Required, String) - Storage backend handle/path
  - `bus` (Optional, String) - Disk bus type (virtio, scsi, ide, sata). Default: virtio
  - `device` (Optional, String) - Device type (disk, cdrom). Default: disk
- `nic` (Optional, List) - Network interface configuration
  - `network` (Required, String) - Network name
  - `model` (Optional, String) - NIC model (virtio, e1000, rtl8139). Default: virtio
  - `mac_address` (Optional, String) - MAC address (auto-generated if not specified)

**Attributes:**

- `id` - VM ID
- `state` - Current state of the VM
- `node_id` - Node ID where the VM is running
- `created_at` - Creation timestamp

### Data Sources

#### kcore_vm

Reads information about an existing VM.

```hcl
data "kcore_vm" "example" {
  id = "vm-123"
}

output "vm_name" {
  value = data.kcore_vm.example.name
}
```

#### kcore_node

Reads information about a specific node.

```hcl
data "kcore_node" "example" {
  id = "node-1"
}

output "node_status" {
  value = data.kcore_node.example.status
}
```

#### kcore_nodes

Lists all nodes in the cluster.

```hcl
data "kcore_nodes" "all" {}

output "all_nodes" {
  value = data.kcore_nodes.all.nodes
}
```

## Examples

See the `examples/` directory for complete examples.

## Development

### Testing

```bash
# Run tests
make tf-test

# Run acceptance tests (requires running KCore controller)
make tf-test-acc
```

### Debugging

To enable debug mode:

```bash
make tf-build
TF_LOG=DEBUG terraform apply
```

## License

See the main KCore project license.

