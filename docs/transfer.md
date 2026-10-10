# Transfer commands

These commands capture and restore paused copies. They do not hand off execution ownership, activate agents, restart services, merge replicas, or restore Herdr layouts.

## Manifest policy

`transfer` is optional. Capture requires an explicit policy, including `transfer: {}` for a code-only project. Unknown fields fail validation.

```yaml
transfer:
  data:
    database:
      location: source
      path: storage/app.sqlite3
      snapshot: {method: sqlite-backup}
    uploads:
      location: runtime
      path: uploads
      snapshot: {method: quiescent-files}
  profiles:
    yuke:
      include: [index.js, plugins]
  required_credentials: [YUKE_PROVIDER_AUTH]
```

Resource names and profile names use DNS-label syntax. Resource paths and profile includes are relative, without traversal or VCS metadata paths. `required` defaults to true. Profile includes cannot overlap. Credential requirements name destination-local capabilities, not credential values or filenames.

Snapshot methods are `sqlite-backup`, `quiescent-files`, and `task`. The task method names declared `capture` and `restore` tasks. Policy validation does not execute them. Task capture/restore and profile capture are not implemented in the checkpoint command; a declared-resource capture rejects either rather than omitting them.

## Preflight

```sh
parcela --path /source transfer plan --to tower \
  --runtime-dir /source-runtime --profile yuke=/source-profile \
  --agent yuke=0123456789abcdef0123456789abcdef
```

Preflight inventories metadata, with a 10,000-entry limit. It does not read payloads, invoke VCS or project commands, connect to SSH, or create registry state. It reports known credential exclusions, links, special files, missing selections, and incomplete inventory. Native Codex/Claude selectors are accepted as explicit intent, not evidence of transfer support. Full handoff readiness remains false.

## Local checkpoints

```sh
parcela --path /source transfer capture --output /private/checkpoint \
  --runtime-dir /source-runtime --source-quiescent
parcela transfer verify /private/checkpoint
parcela transfer restore /private/checkpoint --destination /private/new-replica
```

The output parent must exist. Existing output/restore paths are never overwritten, including empty paths created concurrently. Checkpoint directories use mode 700. Payloads and metadata use mode 600; executable file resources use mode 700. Verification rejects undeclared payloads, links, checksum changes, and executable-mode changes.

Source capture currently requires Git-backed jj. It pins `@`, exports reachable objects through read-only Git plumbing, and restores through jj into an independent colocated repository. Linked workspace pointers are not copied. Source commands disable commit signing for that invocation; they do not change signing configuration or source bookmarks. Known credential-like tracked filenames are rejected, not silently removed from exact source. Filename checks do not establish that arbitrary source/history content is secret-free.

SQLite resources use the online backup API, including committed WAL state, followed by validation. Each database is independently consistent; this is not an atomic multi-database/application checkpoint. File resources require `--source-quiescent`, an operator assertion that their writers are paused. Capture does not stop writers itself.

`--source-only` captures source/history even when data or profiles are declared. Its receipt has `scope: source`, and every data resource remains explicitly uncaptured. Restore preserves that scope. It does not initialize missing application databases or claim that the application can run.

The format version is 1. Agent state, host credentials, process IDs, kernel locks, and service activation are not included. Credentials embedded in declared application data or historical tool output are not generically detected.

## Streaming SQLite snapshots

```sh
parcela transfer stream-sqlite --database /source/app.sqlite3 --to tower \
  --receiver /target/parcela --output /target/private/new-snapshot.sqlite3
```

The receiver parent must already exist, be user-owned, and have mode 700. The target file must be absent. The source opens a pinned read transaction. SQLite's backup API writes through a private destination VFS over SSH; payload bytes are not staged locally. The receiver acknowledges each write with SHA-256, normalizes the unpublished snapshot to rollback-journal mode, validates SQLite, hashes the persisted file, and publishes it without replacement.

The VFS supports one unpublished main database, not a general remote filesystem. It does not support WAL or temporary databases on that destination. The receiver is the only writer. Per-operation buffers are bounded to 4 MiB. Pipe I/O has a one-hour deadline; low source disk space aborts the snapshot to release retained WAL pages. Normal child errors, failed integrity checks, and EOF discard the private partial file. A lost final acknowledgment can leave a valid published snapshot; inspect it instead of overwriting it.

The JSON receipt includes byte count, receiver SHA-256, consistency/transport description, and paused activation. Source and receiver must use matching builds; protocol-version negotiation is not implemented yet. No port forwarding, network listener, CA trust, or app service is started.

`scripts/check_sqlite_stream.py` is the repeatable SSH acceptance helper. It takes explicit `--database NAME=PATH` selections, creates a new private target directory, runs streaming capture, rehashes and validates the persisted snapshots, and writes a private receipt. A partial run retains an incomplete receipt and already verified copies; it is not a complete environment checkpoint.
