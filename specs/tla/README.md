# TLA+ Starter Specs for HA Phase 1

This directory contains a small, bounded TLA+ starter set for the HA rollout.
It is intentionally simple and focused on Phase 1 semantics:

- controller fallback selection
- heartbeat liveness under failover
- eventual replication convergence (bounded)

## Files

- `ControllerNodeReconcile.tla` + `ControllerNodeReconcile.cfg`
  - Node-agent picks an active controller from an ordered list.
  - If active controller is down, it rotates to another reachable controller.
  - Heartbeats only target the active controller.

- `ControllerReplication.tla` + `ControllerReplication.cfg`
  - Two controllers receive events and exchange missing events.
  - Models realtime delivery + anti-entropy pull.
  - Checks eventual set convergence.

- `CrossDcReplication.tla` + `CrossDcReplication.cfg`
  - Three controllers in two DCs.
  - Inter-DC links may flap; anti-entropy eventually repairs divergence.

## Running locally

Example with TLC:

```bash
tlc -config ControllerNodeReconcile.cfg ControllerNodeReconcile.tla
tlc -config ControllerReplication.cfg ControllerReplication.tla
tlc -config CrossDcReplication.cfg CrossDcReplication.tla
```

Notes:

- These specs are intentionally bounded; they are guardrails, not full proofs.
- The model captures protocol-level expectations, not Rust implementation details.
