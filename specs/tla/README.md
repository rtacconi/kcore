# TLA+ Specs for HA Replication

This directory contains bounded TLA+ models for HA/replication behavior.
The intent is to check protocol-level safety/liveness properties with TLC.

- controller fallback/heartbeat progression
- replication convergence under anti-entropy
- cross-DC eventual convergence assumptions

## Files

- `ControllerNodeReconcile.tla` + `ControllerNodeReconcile.cfg`
  - Node-agent picks an active controller by deterministic priority.
  - If active controller becomes unreachable, failover switches to best reachable peer.
  - Heartbeat progression is checked under fair failover/heartbeat scheduling.

- `ControllerReplication.tla` + `ControllerReplication.cfg`
  - Controller-to-controller replication model.
  - Checks event propagation safety and eventual convergence assumptions.

- `CrossDcReplication.tla` + `CrossDcReplication.cfg`
  - Three controllers across two DCs with explicit link state.
  - Separates intra-DC and cross-DC anti-entropy actions.
  - Checks no-double-apply plus eventual cross-DC convergence.

## Running locally

Preferred:

```bash
make test-tla
```

Direct:

```bash
bash ./scripts/check-tla.sh
```

If TLC is not in `PATH`, set one of:

- `TLC_CMD` (full command to run TLC)
- `TLA2TOOLS_JAR` (path to `tla2tools.jar`)

## Trace drift check

Use the trace checker to validate that sampled implementation traces still satisfy
the same deterministic winner and auto-terminal assumptions used in the TLA model:

```bash
make test-tla-trace
```

Sample fixture:

- `specs/tla/traces/replication-sample.json`
- `specs/tla/traces/replication-sample-2.json`
- `specs/tla/traces/replication-invalid-terminal.json` (negative-case fixture)

## CI integration

GitHub Actions workflow `formal-checks.yml` runs:

- `make test-tla-trace` (required)
- `make test-tla` when TLC is available (or when `tla2tools.jar` can be fetched)

Notes:

- These specs are intentionally bounded; they are guardrails, not full proofs.
- The model captures protocol-level expectations, not Rust implementation details.
