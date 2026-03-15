# KCore Terraform Provider

This document provides information about the KCore Terraform provider for infrastructure-as-code management of KCore VMs.

## Overview

The KCore Terraform provider allows you to manage KCore virtual machines using Terraform's declarative configuration language. The provider communicates with the **controller** (not node-agents directly). This enables:

- Infrastructure as Code (IaC) for your VMs
- Version control for your VM configurations
- Automated provisioning and lifecycle management
- Integration with existing Terraform workflows

**Important:** The Terraform provider talks to the kcore controller (default port 9090). The controller then forwards operations to the appropriate node-agent.

## Directory Structure

The Terraform provider is located in the `terraform-provider-kcore/` directory:

```
terraform-provider-kcore/
├── internal/provider/       # Provider implementation
├── examples/                # Example configurations
├── main.go                  # Provider entry point
├── go.mod                   # Go module definition
├── Makefile                 # Build automation
├── README.md                # User documentation
└── DEVELOPMENT.md           # Development guide
```

## Quick Start

### 1. Build and Install the Provider

```bash
cd terraform-provider-kcore
make tf-install
```

This will build the provider and install it to your local Terraform plugins directory.

### 2. Create a Terraform Configuration

Create a new directory for your Terraform configuration:

```bash
mkdir my-kcore-infra
cd my-kcore-infra
```

Create a `main.tf` file:

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

resource "kcore_vm" "example" {
  name         = "my-vm"
  cpu          = 2
  memory_bytes = 4294967296 # 4GB

  disk {
    name           = "root"
    backend_handle = "/var/lib/libvirt/images/my-vm-root.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  nic {
    network = "default"
    model   = "virtio"
  }
}
```

### 3. Initialize and Apply

```bash
terraform init
terraform plan
terraform apply
```

## Features

### Resources

- **kcore_vm**: Manage virtual machines
  - Create, update, and destroy VMs
  - Configure CPU, memory, disks, and network interfaces
  - Optional node targeting for placement

### Data Sources

- **kcore_vm**: Read information about an existing VM
- **kcore_node**: Get details about a specific node
- **kcore_nodes**: List all nodes in the cluster

### Provider Configuration

The provider supports several configuration options:

- **controller_address**: KCore controller gRPC endpoint (default: `localhost:9090`)
- **tls_cert_path**: Path to TLS client certificate
- **tls_key_path**: Path to TLS client key
- **tls_ca_path**: Path to CA certificate
- **insecure**: Disable TLS verification (for development only)

All options can be configured via environment variables:
- `KCORE_CONTROLLER_ADDRESS`
- `KCORE_TLS_CERT`
- `KCORE_TLS_KEY`
- `KCORE_TLS_CA`
- `KCORE_INSECURE`

## Examples

### Basic VM

```hcl
resource "kcore_vm" "web" {
  name         = "web-server"
  cpu          = 2
  memory_bytes = 4294967296

  disk {
    name           = "root"
    backend_handle = "/var/lib/libvirt/images/web-root.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  nic {
    network = "default"
    model   = "virtio"
  }
}
```

### VM with Multiple Disks

```hcl
resource "kcore_vm" "database" {
  name         = "postgres"
  cpu          = 4
  memory_bytes = 8589934592

  disk {
    name           = "root"
    backend_handle = "/var/lib/libvirt/images/postgres-root.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  disk {
    name           = "data"
    backend_handle = "/var/lib/libvirt/images/postgres-data.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  nic {
    network = "default"
    model   = "virtio"
  }
}
```

### Using Data Sources

```hcl
# List all nodes
data "kcore_nodes" "all" {}

output "nodes" {
  value = data.kcore_nodes.all.nodes
}

# Get a specific VM
data "kcore_vm" "existing" {
  id = "vm-123"
}

output "vm_state" {
  value = data.kcore_vm.existing.state
}
```

### Multiple VMs with Count

```hcl
resource "kcore_vm" "web_servers" {
  count = 3

  name         = "web-${count.index + 1}"
  cpu          = 2
  memory_bytes = 4294967296

  disk {
    name           = "root"
    backend_handle = "/var/lib/libvirt/images/web-${count.index + 1}.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  nic {
    network = "default"
    model   = "virtio"
  }
}
```

## Integration with KCore

The Terraform provider communicates with the KCore controller via gRPC. Ensure:

1. The KCore controller is running and accessible
2. The controller address is correctly configured
3. TLS certificates are properly set up (for production)
4. Network connectivity exists between Terraform and the controller

## Development

For information on developing the provider, see:
- [Provider README](../terraform-provider-kcore/README.md)
- [Development Guide](../terraform-provider-kcore/DEVELOPMENT.md)

## Troubleshooting

### Provider Not Found

If Terraform reports the provider is not found:
```bash
cd terraform-provider-kcore
make tf-install
terraform init -upgrade
```

### Connection Errors

If the provider can't connect to the controller:
- Verify the controller is running: `ps aux | grep kcore-controller`
- Check the address: `netstat -an | grep 9090`
- Test connectivity: `grpcurl -plaintext localhost:9090 list`

### TLS Errors

For TLS connection issues:
- Verify certificate paths are correct
- Ensure certificates are not expired
- For development, use `insecure = true`

## Best Practices

1. **Use Version Control**: Store your Terraform configurations in Git
2. **Remote State**: Use remote state backends (S3, Consul, etc.) for team collaboration
3. **Modules**: Create reusable modules for common VM configurations
4. **Variables**: Use variables for environment-specific values
5. **Outputs**: Export important values for use in other configurations
6. **State Locking**: Enable state locking to prevent concurrent modifications
7. **TLS in Production**: Always use TLS in production environments

## Migration from YAML

If you're currently using YAML configuration with `kctl`, you can migrate to Terraform:

**Old YAML (kctl):**
```yaml
apiVersion: kcore.io/v1
kind: VM
metadata:
  name: my-vm
spec:
  cpu: 2
  memoryBytes: 4GiB
  disks:
    - name: root
      sizeBytes: 40GiB
      storageClassName: local-lvm
```

**New Terraform:**
```hcl
resource "kcore_vm" "my_vm" {
  name         = "my-vm"
  cpu          = 2
  memory_bytes = 4294967296

  disk {
    name           = "root"
    backend_handle = "/var/lib/libvirt/images/my-vm-root.qcow2"
    bus            = "virtio"
    device         = "disk"
  }
}
```

## Future Enhancements

Planned features for future releases:
- Support for VM snapshots
- Network resource management
- Storage class resources
- VM templates and cloning
- Auto-scaling groups
- Import existing VMs into Terraform state

## Additional Resources

- [Terraform Documentation](https://www.terraform.io/docs)
- [KCore Architecture](./ARCHITECTURE.md)
- [KCore API Reference](../proto/controller.proto)

