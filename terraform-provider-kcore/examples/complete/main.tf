terraform {
  required_providers {
    kcore = {
      source = "kcore/kcore"
    }
  }
}

provider "kcore" {
  controller_address = var.controller_address
  
  # TLS configuration (recommended for production)
  # tls_cert_path = var.tls_cert_path
  # tls_key_path  = var.tls_key_path
  # tls_ca_path   = var.tls_ca_path
  
  insecure = var.insecure
}

variable "controller_address" {
  description = "Address of the kcore controller"
  type        = string
  default     = "localhost:9090"
}

variable "insecure" {
  description = "Disable TLS verification"
  type        = bool
  default     = true
}

# List all available nodes
data "kcore_nodes" "all" {}

output "available_nodes" {
  description = "List of all available nodes in the cluster"
  value = [
    for node in data.kcore_nodes.all.nodes : {
      id       = node.id
      hostname = node.hostname
      status   = node.status
      cpu      = "${node.cpu_cores_used}/${node.cpu_cores}"
      memory   = "${node.memory_bytes_used}/${node.memory_bytes}"
    }
  ]
}

# Create multiple VMs with different configurations
resource "kcore_vm" "web_servers" {
  count = 3

  name         = "web-server-${count.index + 1}"
  cpu          = 2
  memory_bytes = 4294967296 # 4GB

  disk {
    name           = "root"
    backend_handle = "/var/lib/libvirt/images/web-server-${count.index + 1}-root.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  nic {
    network = "default"
    model   = "virtio"
  }
}

# Create a database VM with more resources
resource "kcore_vm" "database" {
  name         = "postgres-db"
  cpu          = 4
  memory_bytes = 8589934592 # 8GB

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

  # Optional: pin to a specific node
  # target_node = data.kcore_node.primary.id
}

# Output VM information
output "web_servers" {
  description = "Web server VM details"
  value = {
    for vm in kcore_vm.web_servers : vm.name => {
      id      = vm.id
      state   = vm.state
      node_id = vm.node_id
    }
  }
}

output "database" {
  description = "Database VM details"
  value = {
    id      = kcore_vm.database.id
    name    = kcore_vm.database.name
    state   = kcore_vm.database.state
    node_id = kcore_vm.database.node_id
  }
}

# Query a specific VM by ID
data "kcore_vm" "web_server_1" {
  id = kcore_vm.web_servers[0].id
  depends_on = [kcore_vm.web_servers]
}

output "web_server_1_details" {
  description = "Details of the first web server"
  value = {
    name         = data.kcore_vm.web_server_1.name
    cpu          = data.kcore_vm.web_server_1.cpu
    memory_bytes = data.kcore_vm.web_server_1.memory_bytes
    state        = data.kcore_vm.web_server_1.state
    disks        = data.kcore_vm.web_server_1.disk
    nics         = data.kcore_vm.web_server_1.nic
  }
}

