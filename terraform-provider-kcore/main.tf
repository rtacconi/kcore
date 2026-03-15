terraform {
  required_providers {
    kcore = {
      source = "kcore/kcore"
    }
  }
}

provider "kcore" {
  controller_address = "192.168.40.107:9091"

  tls_cert_path = "/mnt/md126/kcore/certs-lenovo-node.crt"
  tls_key_path  = "/mnt/md126/kcore/certs-lenovo-node.key"
  tls_ca_path   = "/mnt/md126/kcore/certs-lenovo-ca.crt"
  insecure      = true  # skip server hostname verification
}

resource "kcore_vm" "my_vm" {
  name         = "debian12-tf"
  cpu          = 1
  memory_bytes = 2147483648  # 2 GiB

  image_uri          = "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2"
  enable_kcore_login = true

  nic {
    network = "default"
    model   = "virtio"
  }
}
