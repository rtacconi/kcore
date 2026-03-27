# Zero External-Resolution Replication Algorithm

This document defines a controller replication algorithm whose normal path requires no manual operator conflict resolution.

## Goal

- Every conflict reaches an automatic terminal outcome in bounded time:
  - `auto_accepted`
  - `auto_rejected`
  - `auto_compensated`
- Operators inspect audit and health signals, but do not perform routine reconciliation.

## Event Model

Every replicated mutation is an envelope with:

- `opId` (unique operation ID)
- `controllerId`
- `dcId`
- `logicalTsUnixMs` (logical ordering timestamp)
- `eventType`
- `resourceKey`
- Optional policy fields:
  - `policyPriority` (default `0`)
  - `intentEpoch` (default `0`)
  - `validity` (`valid`/`invalid`, default `valid`)
  - `safetyClass` (`safe`/`risky`/`unsafe`, default `safe`)
- `body` (event payload)

## Deterministic Arbitration Rank

For two contenders on the same `resourceKey`, the winner is the greater lexicographic rank:

1. `validity` (`valid` > `invalid`)
2. `safetyClass` (`safe` > `risky` > `unsafe`)
3. `policyPriority` (higher wins)
4. `intentEpoch` (higher wins)
5. `logicalTsUnixMs` (higher wins)
6. `controllerId` (string tiebreaker)
7. `opId` (string tiebreaker)

Because rank is total, all controllers converge on the same winner without human input.

## Controller State Machine

`pending` is internal apply processing; terminal states are all automatic.

| Current | Condition | Next | Meaning |
|---|---|---|---|
| `pending` | contender wins rank | `auto_accepted` | winner materialized as head |
| `pending` | contender loses + validity invalid | `auto_rejected` | invalid intent rejected |
| `pending` | contender loses + safety unsafe | `auto_compensated` | compensating action required |
| `pending` | contender loses otherwise | `auto_rejected` | safe deterministic reject |

The loser terminal type is derived from policy class, not operator action.

## Persistence Model

Current tables used by this algorithm:

- `replication_outbox`: emitted events
- `replication_ack`: per-peer frontiers (`pull/*`, `apply/*`, peer ack)
- `replication_received_ops`: dedupe by `opId`
- `replication_resource_heads`: deterministic winner head per resource
- `replication_conflicts`: audit rows for contenders (stored auto-resolved by policy)

## Apply Pipeline

For each incoming event:

1. Parse and validate envelope.
2. Dedupe via `replication_received_ops`.
3. Load current head for `resourceKey`.
4. Compute deterministic rank winner.
5. Upsert `replication_resource_heads` if contender wins.
6. Record audit conflict row with automatic resolution reason.
7. Advance `apply/<peer>` frontier.

No unresolved conflict is required for normal convergence.

## Operator Surface

- Health endpoint reports lag/frontiers and conflict counts.
- Conflict list/resolve APIs remain available as break-glass tooling.
- Normal operations should show zero unresolved conflicts.

## TLA+ Modeling Notes

Define state variables:

- `Outbox`, `Delivered`, `Applied`, `Head`, `ReceivedOps`, `Frontier`, `Conflicts`

Core actions:

- `Emit`, `Deliver`, `ApplyEvent`, `AdvanceFrontier`, `Ack`

Key invariants:

- **DeterministicWinner**: same contenders imply same winner on all controllers.
- **NoDoubleApply**: each `opId` is applied at most once.
- **HeadMonotonicity**: head only changes to a greater rank.
- **NoManualRequired**: conflicts are immediately terminal (`auto_*`) in normal flow.

Liveness:

- Under fair delivery, every delivered event is eventually applied and frontiers converge.

## Current Limitations

- `auto_compensated` terminal semantics are defined but compensation execution is not yet implemented.
- Resource reservations/escrow for scarce resources are not modeled yet.
- Domain materialization from heads to desired-state tables is incremental.
