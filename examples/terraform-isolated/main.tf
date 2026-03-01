terraform {
  required_version = ">= 1.0.0"
  required_providers {
    kcore = {
      source = "registry.opentofu.org/kcore/kcore"
    }
  }
}

provider "kcore" {
  controller_address = var.controller_address
  insecure           = var.insecure
  tls_cert_path      = var.tls_cert_path
  tls_key_path       = var.tls_key_path
  tls_ca_path        = var.tls_ca_path
}

resource "kcore_vm" "test_vm" {
  name         = "${var.vm_name_prefix}-${var.test_id}"
  cpu          = var.cpu
  memory_bytes = var.memory_bytes

  nic {
    network = var.network
    model   = "virtio"
  }
}

output "vm_id" {
  value = kcore_vm.test_vm.id
}

output "vm_name" {
  value = kcore_vm.test_vm.name
}

output "vm_state" {
  value = kcore_vm.test_vm.state
}

output "vm_node_id" {
  value = kcore_vm.test_vm.node_id
}
