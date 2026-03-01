# Quick Start Guide - KCore Terraform Provider

Get started with the KCore Terraform provider in 5 minutes!

## Step 1: Build and Install

```bash
cd terraform-provider-kcore
make tf-install
```

This will build the provider and install it to `~/.terraform.d/plugins/`.

## Step 2: Verify KCore Controller is Running

Ensure your KCore controller is accessible:

```bash
# Check if controller is running
ps aux | grep kcore-controller

# Or start it if needed
cd ..
./bin/kcore-controller
```

## Step 3: Create Your First Terraform Configuration

Create a new directory for your infrastructure:

```bash
mkdir ~/my-kcore-infra
cd ~/my-kcore-infra
```

Create a file named `main.tf`:

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
  insecure           = true  # Only for development!
}

# Create a simple VM
resource "kcore_vm" "my_first_vm" {
  name         = "my-first-vm"
  cpu          = 2
  memory_bytes = 4294967296  # 4GB

  disk {
    name           = "root"
    backend_handle = "/tmp/my-first-vm-root.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  nic {
    network = "default"
    model   = "virtio"
  }
}

# Output the VM ID
output "vm_id" {
  value = kcore_vm.my_first_vm.id
}

output "vm_state" {
  value = kcore_vm.my_first_vm.state
}
```

## Step 4: Initialize Terraform

```bash
terraform init
```

You should see output indicating the KCore provider was successfully initialized.

## Step 5: Plan and Apply

Preview what Terraform will do:

```bash
terraform plan
```

Apply the configuration to create your VM:

```bash
terraform apply
```

Type `yes` when prompted to confirm.

## Step 6: Verify Your VM

Check that your VM was created:

```bash
# List all VMs using the output
terraform output

# Or use kctl if available
../bin/kctl get vm
```

## Step 7: Make Changes

Try modifying your VM. Edit `main.tf` and change the CPU count:

```hcl
resource "kcore_vm" "my_first_vm" {
  name         = "my-first-vm"
  cpu          = 4  # Changed from 2 to 4
  memory_bytes = 4294967296
  # ... rest of config
}
```

See what will change:

```bash
terraform plan
```

Apply the changes:

```bash
terraform apply
```

## Step 8: Query Existing Resources

Add a data source to query your VM:

```hcl
data "kcore_vm" "existing" {
  id = kcore_vm.my_first_vm.id
}

output "vm_details" {
  value = {
    name  = data.kcore_vm.existing.name
    cpu   = data.kcore_vm.existing.cpu
    state = data.kcore_vm.existing.state
  }
}
```

Refresh to see the new outputs:

```bash
terraform refresh
terraform output vm_details
```

## Step 9: Clean Up

When you're done, destroy the VM:

```bash
terraform destroy
```

Type `yes` to confirm deletion.

## Next Steps

Now that you have the basics down, explore:

1. **Multiple VMs**: Use `count` or `for_each` to create multiple VMs
2. **Complete Example**: Check out `examples/complete/` for a more complex setup
3. **Data Sources**: Query nodes and VMs
4. **Modules**: Create reusable modules for your VM configurations
5. **Remote State**: Set up remote state for team collaboration

## Common Commands

```bash
# Initialize Terraform
terraform init

# Preview changes
terraform plan

# Apply changes
terraform apply

# Show current state
terraform show

# List resources
terraform state list

# Destroy everything
terraform destroy

# Format code
terraform fmt

# Validate configuration
terraform validate
```

## Troubleshooting

### Provider not found
```bash
cd terraform-provider-kcore
make tf-install
cd ~/my-kcore-infra
terraform init -upgrade
```

### Can't connect to controller
- Verify controller is running
- Check the address: `localhost:9090`
- Ensure firewall allows connection

### VM creation fails
- Check that the disk paths are writable
- Verify the network exists
- Check controller logs for errors

## Getting Help

- Read the [full README](README.md)
- Check the [Development Guide](DEVELOPMENT.md)
- Review [example configurations](examples/)
- See the [main project docs](../docs/TERRAFORM_PROVIDER.md)

## Example: Complete Infrastructure

Here's a more complete example with multiple VMs:

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

# Web servers
resource "kcore_vm" "web" {
  count = 3

  name         = "web-${count.index + 1}"
  cpu          = 2
  memory_bytes = 4294967296

  disk {
    name           = "root"
    backend_handle = "/tmp/web-${count.index + 1}.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  nic {
    network = "default"
    model   = "virtio"
  }
}

# Database
resource "kcore_vm" "db" {
  name         = "postgres"
  cpu          = 4
  memory_bytes = 8589934592

  disk {
    name           = "root"
    backend_handle = "/tmp/postgres-root.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  disk {
    name           = "data"
    backend_handle = "/tmp/postgres-data.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  nic {
    network = "default"
    model   = "virtio"
  }
}

# Outputs
output "web_servers" {
  value = {
    for vm in kcore_vm.web : vm.name => {
      id    = vm.id
      state = vm.state
    }
  }
}

output "database" {
  value = {
    id    = kcore_vm.db.id
    state = kcore_vm.db.state
  }
}
```

Happy provisioning! 🚀

