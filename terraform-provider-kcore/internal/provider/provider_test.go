package provider

import (
	"os"
	"testing"

	"github.com/hashicorp/terraform-plugin-sdk/v2/helper/schema"
)

// testAccProviders is a map of provider factories for acceptance tests
var testAccProviders map[string]*schema.Provider

// testAccProvider is the provider instance for acceptance tests
var testAccProvider *schema.Provider

func init() {
	testAccProvider = New()
	testAccProviders = map[string]*schema.Provider{
		"kcore": testAccProvider,
	}
}

func TestProvider(t *testing.T) {
	if err := New().InternalValidate(); err != nil {
		t.Fatalf("err: %s", err)
	}
}

func TestProvider_impl(t *testing.T) {
	var _ *schema.Provider = New()
}

func testAccPreCheck(t *testing.T) {
	// Check that KCORE_CONTROLLER_ADDRESS is set
	if v := os.Getenv("KCORE_CONTROLLER_ADDRESS"); v == "" {
		t.Fatal("KCORE_CONTROLLER_ADDRESS must be set for acceptance tests")
	}
}

