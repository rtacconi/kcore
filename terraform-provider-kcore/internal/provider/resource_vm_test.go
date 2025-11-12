package provider

import (
	"fmt"
	"testing"

	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/resource"
	"github.com/hashicorp/terraform-plugin-sdk/v2/terraform"
)

func TestAccKcoreVM_basic(t *testing.T) {
	resourceName := "kcore_vm.test"

	resource.Test(t, resource.TestCase{
		PreCheck:     func() { testAccPreCheck(t) },
		Providers:    testAccProviders,
		CheckDestroy: testAccCheckKcoreVMDestroy,
		Steps: []resource.TestStep{
			{
				Config: testAccKcoreVMConfig_basic(),
				Check: resource.ComposeTestCheckFunc(
					testAccCheckKcoreVMExists(resourceName),
					resource.TestCheckResourceAttr(resourceName, "name", "test-vm"),
					resource.TestCheckResourceAttr(resourceName, "cpu", "2"),
					resource.TestCheckResourceAttr(resourceName, "memory_bytes", "4294967296"),
				),
			},
		},
	})
}

func testAccCheckKcoreVMExists(n string) resource.TestCheckFunc {
	return func(s *terraform.State) error {
		rs, ok := s.RootModule().Resources[n]
		if !ok {
			return fmt.Errorf("Not found: %s", n)
		}

		if rs.Primary.ID == "" {
			return fmt.Errorf("No VM ID is set")
		}

		// Optionally, you could make a GetVM call here to verify it exists

		return nil
	}
}

func testAccCheckKcoreVMDestroy(s *terraform.State) error {
	// Check that all VMs have been destroyed
	for _, rs := range s.RootModule().Resources {
		if rs.Type != "kcore_vm" {
			continue
		}

		// Optionally, verify the VM no longer exists via API call
	}

	return nil
}

func testAccKcoreVMConfig_basic() string {
	return `
resource "kcore_vm" "test" {
  name         = "test-vm"
  cpu          = 2
  memory_bytes = 4294967296

  disk {
    name           = "root"
    backend_handle = "/tmp/test-vm-root.qcow2"
    bus            = "virtio"
    device         = "disk"
  }

  nic {
    network = "default"
    model   = "virtio"
  }
}
`
}
