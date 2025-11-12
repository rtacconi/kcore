terraform {
  required_providers {
    kcore = {
      source = "kcore/kcore"
    }
  }
}

provider "kcore" {
  controller_address = "localhost:9090"
  insecure           = true # Set to false and configure TLS in production
}

# Data source to list all nodes
data "kcore_nodes" "all" {}

# Output all nodes
output "nodes" {
  value = data.kcore_nodes.all.nodes
}

# Data source to get a specific node
data "kcore_node" "example" {
  id = "node-1" # Replace with actual node ID
}

# Create a VM
resource "kcore_vm" "example" {
  name         = "terraform-vm-example"
  cpu          = 2
  memory_bytes = 4294967296 # 4GB

  disk {
    name           = "root"
    backend_handle = "/var/lib/libvirt/images/terraform-vm-root.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  disk {
    name           = "data"
    backend_handle = "/var/lib/libvirt/images/terraform-vm-data.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  nic {
    network = "default"
    model   = "virtio"
  }

  # Optional: specify target node
  # target_node = "node-1"
}

# Output VM information
output "vm_id" {
  value = kcore_vm.example.id
}

output "vm_state" {
  value = kcore_vm.example.state
}

output "vm_node_id" {
  value = kcore_vm.example.node_id
}

# Data source to read an existing VM
data "kcore_vm" "existing" {
  id = kcore_vm.example.id
}

output "existing_vm_name" {
  value = data.kcore_vm.existing.name
}

