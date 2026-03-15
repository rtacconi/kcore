package storage

import (
	"context"
	"testing"
)

type fakeDriver struct {
	name string
}

func (f *fakeDriver) Name() string { return f.name }
func (f *fakeDriver) Create(_ context.Context, _ VolumeSpecOnNode) (string, error) {
	return "/fake/" + f.name, nil
}
func (f *fakeDriver) Delete(_ context.Context, _ string) error              { return nil }
func (f *fakeDriver) Attach(_ context.Context, _, _, _, _ string) error     { return nil }
func (f *fakeDriver) Detach(_ context.Context, _, _ string) error           { return nil }
func (f *fakeDriver) Capacity(_ context.Context) (uint64, uint64, error)    { return 100, 50, nil }

func TestDriverRegistry_RegisterAndGet(t *testing.T) {
	reg := NewDriverRegistry()
	drv := &fakeDriver{name: "test-driver"}
	reg.Register(drv)

	got, err := reg.Get("test-driver")
	if err != nil {
		t.Fatalf("Get: %v", err)
	}
	if got.Name() != "test-driver" {
		t.Errorf("Name()=%q, want test-driver", got.Name())
	}
}

func TestDriverRegistry_GetNotFound(t *testing.T) {
	reg := NewDriverRegistry()
	_, err := reg.Get("nonexistent")
	if err == nil {
		t.Fatal("expected error for missing driver")
	}
}

func TestDriverRegistry_List(t *testing.T) {
	reg := NewDriverRegistry()
	reg.Register(&fakeDriver{name: "alpha"})
	reg.Register(&fakeDriver{name: "beta"})

	names := reg.List()
	if len(names) != 2 {
		t.Fatalf("List() returned %d items, want 2", len(names))
	}

	found := map[string]bool{}
	for _, n := range names {
		found[n] = true
	}
	if !found["alpha"] || !found["beta"] {
		t.Errorf("List()=%v, want alpha and beta", names)
	}
}

func TestDriverRegistry_Empty(t *testing.T) {
	reg := NewDriverRegistry()
	names := reg.List()
	if len(names) != 0 {
		t.Errorf("List() on empty registry=%v, want empty", names)
	}
}
