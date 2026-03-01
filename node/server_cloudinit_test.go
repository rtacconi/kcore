package node

import (
	"strings"
	"testing"
)

func TestDetectImageFlavor(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		imageRef string
		want     string
	}{
		{name: "ubuntu uri", imageRef: "https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.img", want: "ubuntu"},
		{name: "debian uri", imageRef: "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2", want: "debian"},
		{name: "case insensitive", imageRef: "/var/lib/images/Ubuntu-24.04.qcow2", want: "ubuntu"},
		{name: "generic", imageRef: "/var/lib/images/custom-linux.qcow2", want: "generic"},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if got := detectImageFlavor(tt.imageRef); got != tt.want {
				t.Fatalf("detectImageFlavor(%q)=%q want %q", tt.imageRef, got, tt.want)
			}
		})
	}
}

func TestBuildCloudInitUserData_LoginToggleAndFlavor(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name        string
		imageRef    string
		enableLogin bool
		wantFlavor  string
		mustContain []string
		mustNotHave []string
	}{
		{
			name:        "debian login enabled",
			imageRef:    "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2",
			enableLogin: true,
			wantFlavor:  "debian",
			mustContain: []string{
				"root:kcore",
				"kcore:kcore",
				"debian:kcore",
				"ssh_pwauth: True",
				"disable_root: false",
				"serial-getty@ttyS0.service",
			},
			mustNotHave: []string{
				"ubuntu:kcore",
			},
		},
		{
			name:        "ubuntu login enabled",
			imageRef:    "https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img",
			enableLogin: true,
			wantFlavor:  "ubuntu",
			mustContain: []string{
				"root:kcore",
				"kcore:kcore",
				"ubuntu:kcore",
				"ssh_pwauth: True",
				"disable_root: false",
				"serial-getty@ttyS0.service",
			},
			mustNotHave: []string{
				"debian:kcore",
			},
		},
		{
			name:        "debian login disabled",
			imageRef:    "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2",
			enableLogin: false,
			wantFlavor:  "debian",
			mustContain: []string{
				"ssh_pwauth: false",
				"disable_root: true",
				"serial-getty@ttyS0.service",
			},
			mustNotHave: []string{
				"chpasswd:",
				"root:kcore",
				"kcore:kcore",
				"debian:kcore",
				"ubuntu:kcore",
			},
		},
		{
			name:        "ubuntu login disabled",
			imageRef:    "https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.img",
			enableLogin: false,
			wantFlavor:  "ubuntu",
			mustContain: []string{
				"ssh_pwauth: false",
				"disable_root: true",
				"serial-getty@ttyS0.service",
			},
			mustNotHave: []string{
				"chpasswd:",
				"root:kcore",
				"kcore:kcore",
				"debian:kcore",
				"ubuntu:kcore",
			},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			userData, flavor := buildCloudInitUserData(tt.imageRef, tt.enableLogin)
			if flavor != tt.wantFlavor {
				t.Fatalf("flavor=%q want=%q", flavor, tt.wantFlavor)
			}
			for _, s := range tt.mustContain {
				if !strings.Contains(userData, s) {
					t.Fatalf("userData missing required string %q\nuserData:\n%s", s, userData)
				}
			}
			for _, s := range tt.mustNotHave {
				if strings.Contains(userData, s) {
					t.Fatalf("userData unexpectedly contains %q\nuserData:\n%s", s, userData)
				}
			}
		})
	}
}

