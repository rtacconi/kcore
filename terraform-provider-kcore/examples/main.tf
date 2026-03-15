terraform {
  required_providers {
    kcore = {
      source = "kcore/kcore"
    }
  }
}

provider "kcore" {
  controller_address = "localhost:9090"

  tls_cert_path = "/mnt/md126/kcore/certs-lenovo-node.crt"
  tls_key_path  = "/mnt/md126/kcore/certs-lenovo-node.key"
  tls_ca_path   = "/mnt/md126/kcore/certs-lenovo-ca.crt"
  insecure      = true
}

# Create a VM from a cloud image
resource "kcore_vm" "example" {
  name         = "debian12-tf-test"
  cpu          = 1
  memory_bytes = 2147483648 # 2GB

  image_uri          = "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2"
  enable_kcore_login = true

  nic {
    network = "default"
    model   = "virtio"
  }
}

output "vm_id" {
  value = kcore_vm.example.id
}

output "vm_state" {
  value = kcore_vm.example.state
}

output "vm_node_id" {
  value = kcore_vm.example.node_id
}

# Read back the VM we just created
data "kcore_vm" "existing" {
  id = kcore_vm.example.id
}

output "existing_vm_name" {
  value = data.kcore_vm.existing.name
}
