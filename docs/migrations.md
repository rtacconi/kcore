# Database Migrations

kcore-controller uses SQLite for its state store. Schema changes are managed by
a versioned migration system built into `crates/controller/src/db.rs`.

## How it works

Every time the controller opens its database it calls `Database::migrate()`.
That function:

1. Creates all tables with `CREATE TABLE IF NOT EXISTS`, so a brand-new
   database gets the latest schema in one shot.
2. Reads the current version from the `schema_version` table (defaults to `0`
   if the table is empty or missing).
3. Runs each migration block whose version number is higher than the stored
   version. Each block uses `ALTER TABLE … ADD COLUMN` (wrapped in `let _ =`
   so it's a no-op if the column already exists).
4. Writes the new version back to `schema_version`.

```
schema_version
┌─────────┐
│ version │  ← single row, always the latest version number
└─────────┘
```

## Current schema version

**2** (as of March 2026)

## Migration history

| Version | What changed | Why |
|---------|-------------|-----|
| 0 → 1 | Added `image_url`, `image_sha256`, `image_format` columns to `vms`. Backfilled `image_format` from file extension for URL-backed images. | Support downloading VM images by URL with integrity checks. |
| 1 → 2 | Added `runtime_state` column to `vms` (default `'unknown'`). | Persist actual VM state reported by nodes via `SyncVmState`, so the controller doesn't have to poll every node on every `list_vms`. |

## Adding a new migration

1. Bump `CURRENT_VERSION` in `db.rs`.
2. Add a new `if version < N` block **after** the existing ones, containing
   your `ALTER TABLE` or `UPDATE` statements.
3. Keep each migration idempotent — use `let _ =` around `ALTER TABLE` so
   re-running is harmless (SQLite errors on duplicate columns but the error is
   swallowed).
4. Update the table above.
5. Add or update unit tests in `db::tests` that exercise the new column or
   table.

### Example

```rust
// in Database::migrate(), after the version < 2 block:

if version < 3 {
    let _ = conn.execute(
        "ALTER TABLE vms ADD COLUMN ssh_port INTEGER NOT NULL DEFAULT 0",
        [],
    );
}

const CURRENT_VERSION: i32 = 3;  // bump this
```

## Tables overview

### schema_version

Single-row table tracking the migration version.

| Column | Type | Notes |
|--------|------|-------|
| version | INTEGER | Current schema version |

### nodes

Registered cluster nodes.

| Column | Type | Default | Notes |
|--------|------|---------|-------|
| id | TEXT | PK | Node identifier |
| hostname | TEXT | | Reported hostname |
| address | TEXT | | gRPC address (host:port) |
| cpu_cores | INTEGER | 0 | Total CPU cores |
| memory_bytes | INTEGER | 0 | Total memory |
| status | TEXT | `'unknown'` | `ready`, `unknown`, etc. |
| last_heartbeat | TEXT | `''` | ISO 8601 timestamp |
| gateway_interface | TEXT | `''` | NIC used for outbound NAT |

### vms

Virtual machines managed by the controller.

| Column | Type | Default | Added in |
|--------|------|---------|----------|
| id | TEXT | PK | v0 |
| name | TEXT | | v0 |
| cpu | INTEGER | | v0 |
| memory_bytes | INTEGER | | v0 |
| image_path | TEXT | | v0 |
| image_url | TEXT | `''` | v1 |
| image_sha256 | TEXT | `''` | v1 |
| image_format | TEXT | `'raw'` | v1 |
| image_size | INTEGER | 8192 | v0 |
| network | TEXT | `'default'` | v0 |
| auto_start | INTEGER | 1 | v0 |
| node_id | TEXT | FK → nodes | v0 |
| created_at | TEXT | `datetime('now')` | v0 |
| runtime_state | TEXT | `'unknown'` | v2 |

### networks

Per-node network definitions for VM bridges.

| Column | Type | Default | Notes |
|--------|------|---------|-------|
| name | TEXT | PK (with node_id) | Network name |
| external_ip | TEXT | | Public-facing IP for NAT/DNAT |
| gateway_ip | TEXT | | Bridge gateway address |
| internal_netmask | TEXT | `'255.255.255.0'` | Subnet mask |
| node_id | TEXT | PK (with name), FK → nodes | |

## Notes

- The database file lives at the path passed to `Database::open()` (typically
  `/var/lib/kcore/controller.db`).
- WAL mode and foreign keys are enabled on every connection
  (`PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;`).
- Migrations run inside the same connection that opens the database — there is
  no separate migration binary or CLI command.
