variable "controller_address" {
  description = "kcore controller gRPC endpoint"
  type        = string
  default     = "192.168.40.10:9090"
}

variable "insecure" {
  description = "Skip TLS certificate verification (development only)"
  type        = bool
  default     = true
}

variable "tls_cert_path" {
  description = "Client TLS certificate path"
  type        = string
  default     = "../../certs/dev/node.crt"
}

variable "tls_key_path" {
  description = "Client TLS private key path"
  type        = string
  default     = "../../certs/dev/node.key"
}

variable "tls_ca_path" {
  description = "CA certificate path used to verify the controller certificate"
  type        = string
  default     = "../../certs/dev/ca.crt"
}

variable "vm_name_prefix" {
  description = "Prefix for isolated test VM names"
  type        = string
  default     = "tf-debian-lab"
}

variable "test_id" {
  description = "Unique suffix for this isolated test run"
  type        = string
}

variable "cpu" {
  description = "Number of virtual CPUs"
  type        = number
  default     = 2
}

variable "memory_bytes" {
  description = "VM memory in bytes"
  type        = number
  default     = 4294967296 # 4GiB
}

variable "network" {
  description = "libvirt network name on the target node"
  type        = string
  default     = "default"
}
