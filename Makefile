.PHONY: all build check fmt clippy audit lint-nix test test-all test-rust test-nix test-vm test-tla test-tla-trace test-replication-soak coverage test-controller test-node-agent test-kctl test-rust-filter loc iso iso-remote kctl clean install-hooks kani help

VERSION := $(shell cat VERSION)
V ?= v$(VERSION)
SYSTEM ?= $(shell nix eval --impure --raw --expr builtins.currentSystem)

all: build

build:
	cargo build --release

check:
	cargo clippy --all-targets -- --deny warnings
	cargo fmt --check
	cargo audit
	$(MAKE) lint-nix

fmt:
	cargo fmt
	find . -name '*.nix' -not -path './result*' -exec nixfmt {} +

clippy:
	cargo clippy --all-targets -- --deny warnings

audit:
	cargo audit

lint-nix:
	find . -name '*.nix' -not -path './result*' -exec nixfmt --check {} +
	statix check -c .statix.toml .
	deadnix --fail .

test: test-rust

test-all: test-rust test-nix

test-rust:
	cargo test --workspace

test-nix:
	nix flake check

test-vm:
	nix build .#checks.$(SYSTEM).vm-module

test-tla:
	bash ./scripts/check-tla.sh

test-tla-trace:
	bash ./scripts/test-replication-trace.sh

test-replication-soak:
	bash ./scripts/soak-replication.sh

kani:
	@command -v cargo-kani >/dev/null 2>&1 || { \
		echo "cargo-kani not installed."; \
		echo "Install with:  cargo install --locked kani-verifier && cargo kani setup"; \
		exit 1; \
	}
	cargo kani -p kcore-sanitize \
		--jobs "$${KANI_JOBS:-2}" \
		--output-format terse

coverage:
	nix develop -c nix shell nixpkgs#cargo-llvm-cov nixpkgs#cargo nixpkgs#rustc nixpkgs#llvmPackages_21.llvm -c sh -lc 'LLVM_COV="$$(which llvm-cov)" LLVM_PROFDATA="$$(which llvm-profdata)" cargo llvm-cov --workspace --summary-only'

test-controller:
	cargo test -p kcore-controller

test-node-agent:
	cargo test -p kcore-node-agent

test-kctl:
	cargo test -p kcore-kctl

test-rust-filter:
	@if [ -z "$(TEST)" ]; then \
		echo "Usage: make test-rust-filter TEST=<pattern>"; \
		exit 1; \
	fi
	cargo test --workspace "$(TEST)"

loc:
	@echo "Counting source lines..."
	@echo "Rust (.rs): $$(rg --files -g '*.rs' | xargs wc -l | awk 'END {print $$1}')"
	@echo "Nix  (.nix): $$(rg --files -g '*.nix' | xargs wc -l | awk 'END {print $$1}')"

iso:
	@echo "Building kcore ISO $(V) (requires Linux)..."
	nix build .#nixosConfigurations.kcore-iso.config.system.build.isoImage -o result-iso
	@echo ""
	@ls -lh result-iso/iso/*.iso
	@echo ""
	@echo "ISO built: result-iso/iso/kcoreos-$(VERSION)-x86_64-linux.iso"

iso-remote:
	@echo "Building kcore ISO $(V) on remote Linux server..."
	./scripts/build-iso-remote.sh

kctl:
	cargo build --release -p kcore-kctl

install-hooks:
	@for hook in scripts/hooks/*; do \
		name=$$(basename "$$hook"); \
		ln -sf "../../$$hook" ".git/hooks/$$name"; \
		echo "installed .git/hooks/$$name -> $$hook"; \
	done

clean:
	cargo clean
	rm -rf result result-iso dist

help:
	@echo "kcore $(V)"
	@echo ""
	@echo "Targets:"
	@echo "  build       Build all Rust binaries (release)"
	@echo "  check       Run clippy + fmt + audit checks"
	@echo "  fmt         Format Rust and Nix code"
	@echo "  clippy      Run clippy lints"
	@echo "  audit       Run cargo-audit for known vulnerabilities"
	@echo "  lint-nix    Run nixfmt --check, statix, and deadnix on Nix files"
	@echo "  test        Run Rust tests (workspace)"
	@echo "  test-all    Run Rust tests + Nix flake checks"
	@echo "  test-rust   Run all Rust tests in workspace"
	@echo "  test-nix    Run Nix flake checks"
	@echo "  test-vm     Run NixOS VM module test (tests/vm-module.nix)"
	@echo "  test-tla    Run bounded TLC model checks in specs/tla"
	@echo "  test-tla-trace  Run replication trace drift checker"
	@echo "  test-replication-soak  Run bounded replication resilience soak harness"
	@echo "  kani        Run Kani bounded model-checking proofs (requires cargo-kani)"
	@echo "  coverage    Run test coverage via nix develop + cargo-llvm-cov"
	@echo "  test-controller  Run controller crate tests"
	@echo "  test-node-agent  Run node-agent crate tests"
	@echo "  test-kctl   Run kctl crate tests"
	@echo "  test-rust-filter TEST=<pattern>  Run matching Rust tests only"
	@echo "  loc         Count Rust and Nix source lines"
	@echo "  iso         Build NixOS ISO (Linux only)"
	@echo "  iso-remote  Build NixOS ISO on remote Linux server (from macOS)"
	@echo "  kctl        Build kctl CLI only"
	@echo "  install-hooks  Install git pre-commit/pre-push hooks"
	@echo "  clean       Remove build artifacts"
	@echo "  help        Show this help"
