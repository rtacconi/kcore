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

**Target:** same `nixgen.rs` functions, plus disk path validation in `admin.rs`.

**Why second:** Kani gives exhaustive guarantees within bounds, upgrading from "probably correct for random inputs" (proptest) to "provably correct for all inputs up to length N". This matters for security-sensitive escaping.

**Harnesses to write:**

1. **`nix_escape` does not panic** — for all byte sequences up to length 32, `nix_escape` terminates without panic.

2. **`nix_escape` is idempotent on safe strings** — for ASCII alphanumeric input, `nix_escape(s) == s`.

3. **`sanitize_nix_attr_key` output length** — output length equals input length (every character maps to exactly one character).

**Example (Kani):**

```rust
#[cfg(kani)]
mod verification {
    use super::*;

    #[kani::proof]
    #[kani::unwind(34)]
    fn nix_escape_no_panic() {
        let len: usize = kani::any();
        kani::assume(len <= 32);
        let bytes: [u8; 32] = kani::any();
        let s = String::from_utf8_lossy(&bytes[..len]);
        let _ = nix_escape(&s);
    }

    #[kani::proof]
    #[kani::unwind(34)]
    fn sanitize_preserves_length() {
        let len: usize = kani::any();
        kani::assume(len <= 32);
        let bytes: [u8; 32] = kani::any();
        let s = String::from_utf8_lossy(&bytes[..len]);
        let result = sanitize_nix_attr_key(&s);
        assert!(result.len() == s.len());
    }
}
```

**Effort estimate:** 1–2 days. Requires `cargo kani` installed (available via `cargo install kani-verifier && cargo kani setup`). Kani harnesses live alongside the source in `nixgen.rs` behind `#[cfg(kani)]`.

### Phase 3: property-based testing on database invariants

**Target:** `crates/controller/src/db.rs`

**Why third:** the database layer is the source of truth for the entire system. Subtle bugs here (a VM referencing a non-existent node, a heartbeat updating a deleted node) propagate silently and cause wrong Nix configs to be pushed.

**Properties to verify:**

1. **Foreign key integrity** — inserting a `VmRow` with a `node_id` that does not exist in `nodes` must fail.

2. **CRUD round-trip** — for any valid `NodeRow`, `upsert_node` followed by `get_node` returns an identical row.

3. **Delete consistency** — after `delete_vm`, `find_node_for_vm` returns `None` and `list_vms` no longer contains the deleted VM.

4. **Heartbeat idempotence** — calling `update_heartbeat` twice with the same arguments produces the same node state.

5. **Upsert semantics** — upserting a node twice with different addresses updates the address rather than creating a duplicate.

**Effort estimate:** 2–3 days. Requires building `proptest` strategies for `NodeRow` and `VmRow` with constrained string fields.

### Phase 4: TLA+ model of controller–node reconciliation

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

| Phase | Technique | Target | Effort | Confidence gained |
|-------|-----------|--------|--------|-------------------|
| 1 | proptest | `nixgen.rs` escaping and generation | 1–2 days | High — catches injection edge cases |
| 2 | Kani | `nixgen.rs`, disk path validation | 1–2 days | Very high — exhaustive within bounds |
| 3 | proptest | `db.rs` CRUD invariants | 2–3 days | High — catches state consistency bugs |
| 4 | TLA+ | Controller ↔ node reconciliation | 1–2 weeks | Highest — catches distributed protocol bugs |

Phases 1 and 2 can be done in a single iteration. Phase 3 follows naturally. Phase 4 should be started when the reconciliation loop gains retry logic, generation counters, or multi-node rollout sequencing — at that point the complexity will exceed what manual reasoning can reliably cover.
