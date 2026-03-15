package version

import "testing"

func TestVersionDefault(t *testing.T) {
	if Version == "" {
		t.Fatal("Version should not be empty")
	}
	if Version != "dev" {
		t.Logf("Version=%q (overridden from default 'dev')", Version)
	}
}
