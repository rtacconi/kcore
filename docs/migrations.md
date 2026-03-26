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

**6** (as of March 2026)

## Migration history

| Version | What changed | Why |
|---------|-------------|-----|
| 0 → 1 | Added `image_url`, `image_sha256`, `image_format` columns to `vms`. Backfilled `image_format` from file extension for URL-backed images. | Support downloading VM images by URL with integrity checks. |
| 1 → 2 | Added `runtime_state` column to `vms` (default `'unknown'`). | Persist actual VM state reported by nodes via `SyncVmState`, so the controller doesn't have to poll every node on every `list_vms`. |
| 2 → 3 | Added `cpu_used`, `memory_used` to `nodes`; `allowed_tcp_ports`, `allowed_udp_ports` to `networks`; `cloud_init_user_data` to `vms`; new `node_labels` table. | Persist heartbeat resource usage for scheduler, port-forwarding rules per network, cloud-init customization per VM, and node labels for placement hints. |
| 3 → 4 | New `ssh_keys` and `vm_ssh_keys` tables. | SSH public key management — store keys centrally, reference by name when creating VMs, inject into cloud-init. |
| 4 → 5 | Added `storage_backend` to `nodes` and `vms`; added `storage_size_bytes` to `vms`. | Track storage backend preferences per node and VM. |
| 5 → 6 | Added `vlan_id` to `networks`. | 802.1Q VLAN tagging — place VM networks on VLAN sub-interfaces of the upstream NIC. |

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
// in Database::migrate(), after the version < 6 block:

if version < 7 {
    let _ = conn.execute(
        "ALTER TABLE vms ADD COLUMN ssh_port INTEGER NOT NULL DEFAULT 0",
        [],
    );
}

const CURRENT_VERSION: i32 = 7;  // bump this
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
| cpu_used | INTEGER | 0 | CPU cores in use (from heartbeat) |
| memory_used | INTEGER | 0 | Memory in use (from heartbeat) |

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
| cloud_init_user_data | TEXT | `''` | v3 |

### networks

Per-node network definitions for VM bridges.

| Column | Type | Default | Notes |
|--------|------|---------|-------|
| name | TEXT | PK (with node_id) | Network name |
| external_ip | TEXT | | Public-facing IP for NAT/DNAT |
| gateway_ip | TEXT | | Bridge gateway address |
| internal_netmask | TEXT | `'255.255.255.0'` | Subnet mask |
| allowed_tcp_ports | TEXT | `''` | Comma-separated TCP ports for DNAT |
| allowed_udp_ports | TEXT | `''` | Comma-separated UDP ports for DNAT |
| vlan_id | INTEGER | `0` | 802.1Q VLAN tag (0 = no VLAN) |
| node_id | TEXT | PK (with name), FK → nodes | |

### node_labels

Per-node labels for placement hints and metadata.

| Column | Type | Notes |
|--------|------|-------|
| node_id | TEXT | PK (with label), FK → nodes |
| label | TEXT | PK (with node_id), e.g. `dc=dc-a` |

### ssh_keys

Centrally stored SSH public keys, referenced by name.

| Column | Type | Default | Notes |
|--------|------|---------|-------|
| name | TEXT | PK | Human-readable key name |
| public_key | TEXT | | OpenSSH public key string |
| created_at | TEXT | `datetime('now')` | When the key was added |

### vm_ssh_keys

Association table linking VMs to SSH keys for cloud-init injection.

| Column | Type | Notes |
|--------|------|-------|
| vm_id | TEXT | PK (with key_name), FK → vms ON DELETE CASCADE |
| key_name | TEXT | PK (with vm_id), FK → ssh_keys |

## Notes

- The database file lives at the path passed to `Database::open()` (typically
  `/var/lib/kcore/controller.db`).
- WAL mode and foreign keys are enabled on every connection
  (`PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;`).
- Migrations run inside the same connection that opens the database — there is
  no separate migration binary or CLI command.
