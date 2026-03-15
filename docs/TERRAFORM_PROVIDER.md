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
- **kcore_cluster_ca**: Manage cluster CA (generates CA cert/key pair)
- **kcore_node_install**: Install NixOS on a bare-metal node (multi-disk, controller role)
- **kcore_node_network**: Configure node networking (bridges, bonds, VLANs)
- **kcore_node_nixconfig**: Push NixOS configuration to a node

### Data Sources

- **kcore_vm**: Read information about an existing VM
- **kcore_node**: Get details about a specific node
- **kcore_nodes**: List all nodes in the cluster
- **kcore_node_disks**: List block devices on a node
- **kcore_node_nics**: List network interfaces on a node

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

## Cluster & Node Provisioning Resources

### kcore_cluster_ca

Generates and manages the cluster CA. The CA key is stored in Terraform state -- use a secure backend.

```hcl
resource "kcore_cluster_ca" "main" {
  cluster_name = "prod"
  base_path    = "~/.kcore"
}

output "ca_cert" {
  value = kcore_cluster_ca.main.ca_cert
}
```

| Attribute | Type | Description |
|---|---|---|
| `cluster_name` | string | Cluster identifier |
| `base_path` | string | Local directory for CA files (default: `~/.kcore`) |
| `ca_cert` | string (read) | PEM-encoded CA certificate |

### kcore_node_install

Installs NixOS on a node with multi-disk and optional controller role.

```hcl
data "kcore_node_disks" "n1" {
  node_id = "node-abc123"
}

resource "kcore_node_install" "n1" {
  node_id       = "node-abc123"
  hostname      = "kcore01"
  root_password = "s3cret"

  disk {
    device = "/dev/sda"
    role   = "os"
  }

  disk {
    device = "/dev/nvme0n1"
    role   = "storage"
  }

  run_controller     = true
  controller_address = "10.0.0.1:9090"
}
```

| Attribute | Type | Description |
|---|---|---|
| `node_id` | string | Target node identifier |
| `hostname` | string | Hostname for installed system |
| `root_password` | string | Root password (sensitive) |
| `disk` | block list | Disks to use (`device`, `role`: `os` or `storage`) |
| `run_controller` | bool | Deploy controller on this node (default: false) |
| `controller_address` | string | Advertised controller address |

### kcore_node_network

Configures networking on an installed node.

```hcl
resource "kcore_node_network" "n1" {
  node_id = "node-abc123"

  bridge {
    name  = "br0"
    ports = ["enp1s0"]
  }

  bond {
    name       = "bond0"
    mode       = "802.3ad"
    interfaces = ["enp2s0", "enp3s0"]
  }

  vlan {
    name   = "vlan100"
    id     = 100
    parent = "bond0"
  }

  dns_servers = ["1.1.1.1", "8.8.8.8"]
  apply_now   = true
}
```

| Attribute | Type | Description |
|---|---|---|
| `node_id` | string | Target node identifier |
| `bridge` | block list | Bridge definitions (`name`, `ports[]`) |
| `bond` | block list | Bond definitions (`name`, `mode`, `interfaces[]`) |
| `vlan` | block list | VLAN definitions (`name`, `id`, `parent`) |
| `dns_servers` | list(string) | DNS server addresses |
| `apply_now` | bool | Apply immediately (default: false) |

### kcore_node_nixconfig

Pushes a NixOS configuration file to a node.

```hcl
resource "kcore_node_nixconfig" "n1" {
  node_id   = "node-abc123"
  nix_config = file("${path.module}/configuration.nix")
  rebuild    = true
}
```

| Attribute | Type | Description |
|---|---|---|
| `node_id` | string | Target node identifier |
| `nix_config` | string | NixOS configuration content |
| `rebuild` | bool | Run `nixos-rebuild switch` after push (default: false) |

### Data Sources: Node Hardware

```hcl
data "kcore_node_disks" "n1" {
  node_id = "node-abc123"
}

output "disks" {
  value = data.kcore_node_disks.n1.disks
  # Each disk: { device, size_bytes, model, serial, in_use }
}

data "kcore_node_nics" "n1" {
  node_id = "node-abc123"
}

output "nics" {
  value = data.kcore_node_nics.n1.nics
  # Each nic: { name, mac, speed, state, driver }
}
```

### Full Provisioning Example

```hcl
resource "kcore_cluster_ca" "main" {
  cluster_name = "prod"
}

data "kcore_node_disks" "n1" {
  node_id = "node-abc123"
}

resource "kcore_node_install" "n1" {
  node_id  = "node-abc123"
  hostname = "kcore01"

  disk {
    device = "/dev/sda"
    role   = "os"
  }

  disk {
    device = "/dev/nvme0n1"
    role   = "storage"
  }

  run_controller     = true
  controller_address = "10.0.0.1:9090"
}

resource "kcore_node_network" "n1" {
  node_id = kcore_node_install.n1.node_id

  bridge {
    name  = "br0"
    ports = ["enp1s0"]
  }

  dns_servers = ["1.1.1.1"]
  apply_now   = true
}

resource "kcore_node_nixconfig" "n1" {
  node_id    = kcore_node_install.n1.node_id
  nix_config = file("${path.module}/configuration.nix")
  rebuild    = true
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

