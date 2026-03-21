.PHONY: all build check fmt clippy iso iso-remote kctl clean help

VERSION := $(shell cat VERSION)
V ?= v$(VERSION)

all: build

build:
	cargo build --release

check:
	cargo clippy --all-targets -- --deny warnings
	cargo fmt --check

fmt:
	cargo fmt

clippy:
	cargo clippy --all-targets -- --deny warnings

iso:
	@echo "Building kcore ISO $(V) (requires Linux)..."
	nix build .#nixosConfigurations.kcore-iso.config.system.build.isoImage -o result-iso
	@echo ""
	@ls -lh result-iso/iso/*.iso
	@echo ""
	@echo "ISO built: result-iso/iso/nixos-kcore-$(VERSION)-x86_64-linux.iso"

iso-remote:
	@echo "Building kcore ISO $(V) on remote Linux server..."
	./scripts/build-iso-remote.sh

kctl:
	cargo build --release -p kcore-kctl

clean:
	cargo clean
	rm -rf result result-iso dist

help:
	@echo "kcore $(V)"
	@echo ""
	@echo "Targets:"
	@echo "  build       Build all Rust binaries (release)"
	@echo "  check       Run clippy + fmt check"
	@echo "  fmt         Format Rust code"
	@echo "  clippy      Run clippy lints"
	@echo "  iso         Build NixOS ISO (Linux only)"
	@echo "  iso-remote  Build NixOS ISO on remote Linux server (from macOS)"
	@echo "  kctl        Build kctl CLI only"
	@echo "  clean       Remove build artifacts"
	@echo "  help        Show this help"
