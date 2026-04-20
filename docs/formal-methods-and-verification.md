# Formal Methods and Verification

This document evaluates how formal verification and related techniques can improve correctness in kcore-rust, and lays out a practical roadmap for adopting them incrementally.

## Why consider formal methods here

Rust's type system already eliminates entire classes of bugs (use-after-free, data races, null dereferences). The kcore-rust codebase contains no `unsafe` code, so those guarantees hold end-to-end at the source level. What Rust does _not_ catch:

- **Logic bugs** in string generation (Nix injection through a missed escape sequence)
- **Invalid state transitions** (a node marked `ready` when it should be `unknown`, a VM persisted without a valid node)
- **Distributed protocol errors** (controller pushes config, node reboots mid-rebuild, controller retries on stale state)
- **Scheduler invariants** (a VM placed on a node that cannot satisfy resource constraints)

These are the gaps where formal methods add value.

## What Rust already gives us

Before adding any tooling, it is worth recognizing what the compiler enforces for free:

| Guarantee | Mechanism |
|-----------|-----------|
| No data races | Ownership + `Send`/`Sync` traits |
| No use-after-free | Borrow checker |
| No null pointer dereferences | `Option<T>` instead of null |
| No buffer overflows | Bounds-checked indexing |
| Mutex poisoning handled | `lock_conn()` returns `Err` instead of panicking |

This is a form of lightweight formal verification. Every `cargo build` is a proof run.

## Applicable tools

### Kani (bounded model checking)

[Kani](https://github.com/model-checking/kani) is Amazon's model checker for Rust. You write proof harnesses that look like tests, but Kani exhaustively explores all possible inputs within declared bounds instead of running a single concrete case.

**Strengths:** finds edge cases that unit tests miss, proves absence of panics, integrates into `cargo`.

**Limitations:** no async/tokio support, no reasoning about external I/O, bounded (you pick the input size limits).

**Where it applies in kcore-rust:**

- `nix_escape` and `sanitize_nix_attr_key` in `crates/controller/src/nixgen.rs`
- `select_node` in `crates/controller/src/scheduler.rs`
- Disk path validation logic in `crates/node-agent/src/grpc/admin.rs`

### proptest (property-based testing)

[proptest](https://github.com/proptest-rs/proptest) generates random inputs guided by strategies and checks that properties hold across thousands of cases. Not formally "formal methods", but in practice it catches the same class of bugs as bounded model checking for string-processing and data-transformation code, with much less setup.

**Strengths:** easy to add alongside existing tests, excellent for string/parsing code, well-maintained.

**Limitations:** probabilistic (not exhaustive), cannot prove absence of bugs.

**Where it applies in kcore-rust:**

- Nix generation round-trip properties
- Database CRUD invariants
- Config file serialization/deserialization

### TLA+ (design-level model checking)

[TLA+](https://lamport.azurewebsites.net/tla/tla.html) models distributed protocols as state machines and exhaustively checks safety and liveness properties. It operates on the design, not the code — you write a separate specification and verify it independently.

**Strengths:** catches distributed systems bugs that no amount of unit testing will find, widely used in industry (AWS, Azure, CockroachDB).

**Limitations:** requires learning a separate language, does not verify Rust code directly, the model must be kept in sync with the implementation manually.

**Where it applies in kcore-rust:**

- Controller ↔ node-agent reconciliation loop
- Failure scenarios during `nixos-rebuild switch`
- Scheduler placement and node lifecycle transitions

For the HA-specific replication and datacenter plan that will be modeled with TLA+, see:

- `docs/ha-crdt-replication.md`
- `specs/tla/` (starter Phase 1 model files and TLC configs)

### Prusti and Creusot (deductive verification)

These tools add pre/post-conditions and loop invariants to Rust code and verify them statically. Prusti translates to Viper, Creusot to Why3.

**Assessment:** not practical for this project today. Both tools struggle with async code, external crate calls (`tonic`, `rusqlite`, `hyper`), and string manipulation. The effort required to annotate the codebase would vastly exceed the value gained. Revisit only if kcore-rust grows a substantial pure algorithmic core (e.g., a constraint-based scheduler).

## Roadmap

### Phase 1: property-based testing on Nix generation

**Target:** `crates/controller/src/nixgen.rs`

**Why first:** this code produces Nix expressions that run as root during `nixos-rebuild switch`. A missed escape sequence is a privilege escalation bug. The functions are pure (no I/O, no async), making them ideal candidates.

**Properties to verify:**

1. **Escape round-trip safety** — for any input string `s`, `nix_escape(s)` never contains an unescaped `"`, an unescaped `\`, or an unescaped `${` sequence.

2. **Sanitization completeness** — for any input string `s`, every character in `sanitize_nix_attr_key(s)` is in `[a-zA-Z0-9_-]`.

3. **Structural validity** — for any list of `VmRow` values with arbitrary string fields, `generate_node_config` produces output that starts with `{ pkgs, ... }: {` and ends with `}\n`, and contains balanced braces.

4. **No interpolation breakout** — for any `VmRow` with `image_path` or `name` containing `"; malicious-code "`, the generated config does not contain unescaped user content outside of Nix string literals.

**Example (proptest):**

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn nix_escape_never_contains_unescaped_dollar_brace(s in ".*") {
        let escaped = nix_escape(&s);
        // Walk the escaped string: every `${` must be preceded by `\`
        let bytes = escaped.as_bytes();
        for i in 0..bytes.len().saturating_sub(1) {
            if bytes[i] == b'$' && bytes[i + 1] == b'{' {
                assert!(i > 0 && bytes[i - 1] == b'\\',
                    "unescaped ${{ at position {i} in escaped string: {escaped}");
            }
        }
    }

    #[test]
    fn sanitize_nix_attr_key_only_safe_chars(s in "\\PC{1,100}") {
        let sanitized = sanitize_nix_attr_key(&s);
        assert!(sanitized.chars().all(|c|
            c.is_ascii_alphanumeric() || c == '-' || c == '_'
        ));
    }
}
```

**Effort estimate:** 1–2 days to add `proptest` as a dev dependency and write 4–6 property tests.

### Phase 2: Kani harnesses for critical pure functions

**Status:** shipped. Every bounded model-checking proof lives in a single dedicated crate, [`crates/kcore-sanitize`](../crates/kcore-sanitize/src/lib.rs), which contains:

- `nix_escape` and `sanitize_nix_attr_key` — Nix string-literal escaping.
- `path_segments_include_dot_dot` and `assert_safe_path` — generic path-traversal guards used by `kcore-controller` and `kcore-kctl`.
- `validate_safe_segment` and `validate_path_under_root` — node-agent-side guards for tenant-supplied path segments and absolute paths.

`kcore-controller`, `kcore-kctl` and `kcore-node-agent` all delegate to this crate via thin wrappers in their own `path_safety.rs` / `nixgen.rs`, preserving their existing public APIs (which embed a `label` for human-readable error messages) without duplicating the validators.

**Why a separate crate?** Kani compiles every crate it analyses through its own `goto-c` `rustc` wrapper. Pointing `cargo kani` at `kcore-controller` (which transitively depends on rusqlite-bundled SQLite, rcgen+aws-lc-rs, tonic, rustls, x509-parser …) takes >20 minutes on the 4-vCPU GitHub Actions runner before any proof runs. `kcore-sanitize` has **zero non-std dependencies**, so the same proofs finish in seconds. All harnesses are gated behind `#[cfg(kani)]`, so they are excluded from `cargo build`, `cargo test`, and `cargo clippy` and have **zero impact on the stable rustc toolchain**.

**Why this layer:** Kani gives exhaustive guarantees within bounds, upgrading from "probably correct for 2 000 random inputs" (proptest) to "provably correct for **all** ASCII inputs up to `MAX_INPUT_LEN` bytes" (currently 4). This matters for security-sensitive escaping where a single missed byte enables arbitrary Nix evaluation or directory traversal.

**Harnesses currently proven:**

1. **`nix_escape` never panics** — for all ASCII strings up to `MAX_INPUT_LEN` bytes.
2. **`nix_escape` output is always safely escaped** — soundness of the security boundary.
3. **`sanitize_nix_attr_key` preserves char count** — 1-to-1 mapping property.
4. **`sanitize_nix_attr_key` charset** — output is in `[A-Za-z0-9_-]`.
5. **`path_segments_include_dot_dot` never panics** — total function on bounded input.
6. **`assert_safe_path` never panics** — total function on bounded input.
7. **`assert_safe_path` acceptance is sound** — accepted inputs are non-empty, NUL-free, and contain no `..` segment under either separator.
8. **`validate_safe_segment` never panics** — total function on bounded input.
9. **`validate_safe_segment` acceptance is sound** — accepted segments are non-empty, NUL-free, separator-free, not `.`/`..`, no leading `-`.

**How to run locally:**

```bash
cargo install --locked kani-verifier --version 0.67.0
cargo kani setup
make kani                 # cargo kani -p kcore-sanitize --jobs 2 --output-format terse
# locally, override parallelism with KANI_JOBS=4 make kani if you have memory headroom
```

**CI:** the `kani-shard` matrix job in `.github/workflows/formal-checks.yml` pins `kani-verifier` to a known version (`KANI_VERSION` env var, currently `0.67.0`) and fans the 9 `#[kani::proof]` harnesses out across one runner per harness. The aggregator job `kani` (`needs: [kani-shard]`) keeps the stable `formal-checks / kani` status check green for branch-protection.

Why per-harness sharding rather than a single job with `--jobs N`:

1. Wall-clock parallelism is much higher (~9× vs sequential), so even the slowest harness no longer gates the rest.
2. Each runner verifies exactly one harness, so the 4 vCPU / 16 GB `ubuntu-latest` budget is never threatened by multiple in-flight CBMC processes (peak RAM is the single harness's footprint, typically <4 GB).
3. A regression in one harness fails just that shard rather than masking later harnesses behind it.

Two non-obvious CI requirements that bit earlier single-job attempts:

1. `--jobs` was stabilised in `kani-verifier` 0.63, so it no longer needs (and on newer Kani it actively rejects) `-Z unstable-options`. With per-harness sharding we no longer use `--jobs` in CI at all.
2. `--jobs N` with `N > 1` previously needed `--output-format terse` (`Conflicting options: --jobs requires '--output-format=terse'`). We keep `--output-format terse` in the matrix step anyway because it produces parseable, line-oriented logs.

**Cache strategy:** the toolchain cache (`~/.cargo/bin/cargo-kani`, `~/.cargo/bin/kani`, `~/.kani/`) is keyed only on `KANI_VERSION` (NOT on `matrix.harness`) so all 9 shards share the same cache slot. The cache is saved **immediately after install completes**, before the proof step runs, because GitHub Actions does not execute steps after a job-level cancellation (manual Cancel, OOM, or hard timeout) even when guarded with `if: always()`. On a cold cache, the 9 shards race to install and then race to save under the same key; GHA serialises the saves and the first one wins (the others log a no-op warning, masked with `continue-on-error`). Every subsequent run on the same `KANI_VERSION` skips install entirely (~30 s vs ~5–10 min).

**Per-harness performance:** the slowest harness historically was `dot_dot_check_never_panics` (~9 min CBMC time) because `path_segments_include_dot_dot` used `path.split([…]).any(…)`, which forced Kani to symbolically execute Rust's `core::str::pattern` machinery. It was rewritten as a flat byte loop (functionally equivalent for the ASCII separators `/` and `\\`), bringing the harness back into the seconds range and making the matrix wall-clock budget realistic.

**Follow-up work:**

- Raise `MAX_INPUT_LEN` once we've measured the runtime cost on CI.

### Phase 3: property-based testing on database invariants

**Status:** shipped. Bounded `proptest` harnesses live in `crates/controller/src/db.rs` (`mod proptests`, ~256 randomised cases each, all running on `:memory:` SQLite).

**Properties currently checked:**

1. **Node CRUD round-trip** — `upsert_node` then `get_node` returns a structurally equal row across randomised id/hostname/address/cpu/memory/cert/dc fields.
2. **Upsert idempotence** — two upserts of the same node produce a single `list_nodes` row.
3. **Upsert updates fields in place** — two upserts with different `address` produce one row with the latest address.
4. **Foreign-key integrity** — inserting a `VmRow` whose `node_id` does not exist in `nodes` MUST fail (relies on `PRAGMA foreign_keys=ON`).
5. **VM CRUD round-trip** — `insert_vm` then `get_vm` returns matching scalar fields.
6. **Delete consistency** — after `delete_vm_by_id_or_name`, the VM disappears from `get_vm`, `find_node_for_vm`, and `list_vms`.
7. **Heartbeat idempotence (modulo timestamp)** — two `update_heartbeat` calls with identical args produce identical state in every field except `last_heartbeat`.

**Target:** `crates/controller/src/db.rs` (`#[cfg(test)] mod proptests`).

**Why third:** the database layer is the source of truth for the entire system. Subtle bugs here (a VM referencing a non-existent node, a heartbeat updating a deleted node) propagate silently and cause wrong Nix configs to be pushed.

**How to run locally:**

```bash
cargo test -p kcore-controller proptests
```

### Phase 4: TLA+ model of controller–node reconciliation

**Status:** shipped. Bounded TLC model checks run as a required CI gate in `.github/workflows/formal-checks.yml`. The specs live in `specs/tla/`:

- `ControllerNodeReconcile.tla` — controller ↔ node reconciliation.
- `ControllerReplication.tla` — single-DC replication protocol.
- `CrossDcReplication.tla` — multi-DC replication with safety **and** liveness properties; the latest local run explored 206 280 distinct states across 3 941 400 generated states with no error found.

Run locally:

```bash
make test-tla            # bounded TLC checks (requires java + tla2tools.jar)
make test-tla-trace      # replication trace drift checker
```

The trace bridge in `make test-tla-trace` validates Rust runtime traces against the TLA+ invariants, closing the gap between the model and the code.

**Target:** the distributed protocol between `kcore-controller` and `kcore-node-agent`.

**Why last:** this is the highest-value verification target long-term (distributed bugs are the hardest to test and reproduce), but also the highest-effort one. It requires learning TLA+ and maintaining a model separate from the code.

**What to model:**

```
Variables:
  controller_desired_state : NodeId -> Set(VmConfig)
  node_applied_state       : NodeId -> Set(VmConfig)
  node_status              : NodeId -> {unknown, ready, rebuilding, unreachable}
  pending_applies          : Set(ApplyRequest)

Safety properties:
  - Eventually, node_applied_state converges to controller_desired_state
    for all reachable nodes (convergence)
  - A node never applies a config that references a deleted VM (no stale apply)
  - Two concurrent applies to the same node are serialized (no config race)

Liveness properties:
  - If a node becomes unreachable and then recovers, reconciliation
    eventually resumes
  - If the controller crashes and restarts, no VM is orphaned
    (desired state survives in SQLite)

Failure injections to explore:
  - Node reboots mid-rebuild (nixos-rebuild killed by SIGTERM)
  - Controller crashes after persisting VM but before pushing config
  - Network partition between controller and node during apply
  - Node returns to ready but with stale config from a previous generation
```

**Effort estimate:** 1–2 weeks for a first useful model. The TLA+ specification lives in a `specs/` directory and is checked with the TLC model checker, independent of `cargo`. The value compounds as the reconciliation loop grows more complex (retries, generation counters, multi-node rollouts).

## What is not worth verifying formally

Most of the codebase (~80%) is I/O glue: gRPC request handlers, file system writes, subprocess spawning, HTTP calls to Cloud Hypervisor sockets. Formal methods tools cannot reason about these external systems, and the bugs that occur there are better caught by integration tests (the existing NixOS VM test in `tests/vm-module.nix` is a good example).

Specifically, do not invest formal verification effort in:

- `tonic` gRPC handler boilerplate
- `tokio::process::Command` calls
- `hyper`/`hyperlocal` HTTP client code in `vmm/client.rs`
- Config file parsing (`serde` deserialization)
- CLI argument handling in `kctl`

## Summary

| Phase | Technique | Target | Status | Confidence gained |
|-------|-----------|--------|--------|-------------------|
| 1 | proptest | `nixgen.rs` escaping and generation | **Shipped** | High — catches injection edge cases |
| 2 | Kani | `nixgen.rs`, path-safety validators | **Shipped (first cut)** | Very high — exhaustive within bounds |
| 3 | proptest | `db.rs` CRUD invariants | **Shipped** | High — catches state consistency bugs |
| 4 | TLA+ | Controller ↔ node reconciliation | **Shipped** | Highest — catches distributed protocol bugs |

All four phases are now landed. Each one is wired into CI (`make test`, `make kani`, `make test-tla`, `make test-tla-trace`) so a regression on any property fails the pull-request check.

**Next-iteration follow-ups (not blockers):**

- Raise the Kani `MAX_INPUT_LEN` bound once we've measured CI runtime cost.
- Add a Kani harness for `validate_path_under_root` (its sibling `validate_safe_segment` is already proved in `crates/kcore-sanitize/src/lib.rs::kani_proofs`).
- Extend Phase 3 proptests to cover replication outbox CRUD and security-group rule round-trips.
- Add TLA+ invariants for the new compensation executor and replication reservations.
