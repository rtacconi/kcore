# Database Migrations

Both the controller and node-agent use SQLite for state persistence with versioned migrations.

## Migration Pattern

Each database has a `schema_version` table that tracks the current schema version. Migrations are numbered functions that run in order.

```go
var migrations = []migration{
    {"001_initial", migration001Initial},
    {"002_desired_state", migration002DesiredState},
    // add new migrations here
}
```

On startup, the `migrate()` method:

1. Creates the `schema_version` table if it doesn't exist
2. Reads the current version
3. Runs any migrations after the current version, each in its own transaction
4. Updates `schema_version` after each successful migration

## Adding a New Migration

1. Create a new function following the naming convention:

```go
func migration003MyChange(tx *sql.Tx) error {
    _, err := tx.Exec(`ALTER TABLE vms ADD COLUMN new_field TEXT`)
    return err
}
```

2. Append it to the `migrations` slice:

```go
var migrations = []migration{
    {"001_initial", migration001Initial},
    {"002_desired_state", migration002DesiredState},
    {"003_my_change", migration003MyChange},
}
```

3. Add tests for the new migration.

## Rules

- **Never use `CREATE TABLE IF NOT EXISTS` for new schema.** Always add a new numbered migration function.
- **Each migration runs in a transaction.** If it fails, the transaction rolls back and the schema version is not updated.
- **Migrations are idempotent.** Running `migrate()` multiple times is safe -- already-applied migrations are skipped.
- **Forward-only.** There are no down migrations. If you need to undo a change, create a new migration that reverses it.

## Controller Database

Location: `--db` flag (default: `./kcore-controller.db`)

Tables (after all migrations):
- `schema_version` -- migration tracking
- `nodes` -- registered node-agents
- `storage_classes` -- storage class definitions
- `volumes` -- provisioned volumes
- `vms` -- VM records with desired_spec, desired_state, image_uri
- `vm_disks` -- VM disk attachments
- `vm_nics` -- VM network interfaces
- `vm_placement` -- desired vs actual placement and state
- `networks` -- network definitions

## Node-Agent Database

Location: configured at startup (default: `/var/lib/kcore/node.db`)

Tables (after all migrations):
- `schema_version` -- migration tracking
- `vm_metadata` -- VM metadata that libvirt doesn't store (image URI, cloud-init config)
- `cached_images` -- downloaded image cache records
- `operation_log` -- audit trail of operations

## Testing

```bash
# Run SQLite migration tests
go test ./pkg/sqlite/... -v -count=1

# Run node-agent DB tests
go test ./node/... -v -count=1 -run NodeDB
```
