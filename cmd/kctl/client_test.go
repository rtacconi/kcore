package main

import (
	"testing"
)

func TestParseMemorySize(t *testing.T) {
	t.Parallel()

	tests := []struct {
		input   string
		want    int64
		wantErr bool
	}{
		{"1G", 1073741824, false},
		{"2G", 2147483648, false},
		{"512M", 536870912, false},
		{"1024M", 1073741824, false},
		{"4G", 4294967296, false},
		{"", 0, true},
		{"abc", 0, true},
		{"G", 0, true},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.input, func(t *testing.T) {
			t.Parallel()
			got, err := parseMemorySize(tt.input)
			if tt.wantErr {
				if err == nil {
					t.Fatal("expected error")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got != tt.want {
				t.Fatalf("parseMemorySize(%q)=%d, want=%d", tt.input, got, tt.want)
			}
		})
	}
}

func TestFormatBytes(t *testing.T) {
	t.Parallel()

	tests := []struct {
		input int64
		want  string
	}{
		{1073741824, "1.0 GB"},
		{2147483648, "2.0 GB"},
		{536870912, "512.0 MB"},
		{4294967296, "4.0 GB"},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.want, func(t *testing.T) {
			t.Parallel()
			got := formatBytes(tt.input)
			if got != tt.want {
				t.Fatalf("formatBytes(%d)=%q, want=%q", tt.input, got, tt.want)
			}
		})
	}
}
