# Capture and restore paused checkpoints

These commands create paused copies. They do not transfer active execution, activate an agent, restart services, merge replicas or restore terminal layouts.

## Declare the capture policy

Add an explicit `transfer` policy to the manifest. For a code-only checkpoint, declare `transfer: {}`. For application resources, state each relative source or runtime path and its supported snapshot method:

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
```

Do not include credentials or assume that filename filtering makes arbitrary source/history/data secret-free. Resource paths cannot traverse outside their declared roots. Profile capture and task-based capture/restore are not implemented; declared-resource capture refuses those policies rather than omitting them.

## Inspect without capture

```sh
parcela --path "$SOURCE" transfer plan --to "$DESTINATION" \
  --runtime-dir "$SOURCE_RUNTIME"
```

Preflight inventories metadata. It does not read payloads, connect to SSH, execute project commands or create registry state. Its bounded inventory and reported selections do not establish full handoff readiness.

## Capture a local paused copy

Choose a new private output outside the checkout and selected resource paths. Its parent must already exist. Pause selected file writers yourself before asserting quiescence:

```sh
parcela --path "$SOURCE" transfer capture --output "$CHECKPOINT" \
  --runtime-dir "$SOURCE_RUNTIME" --source-quiescent
parcela transfer verify "$CHECKPOINT"
```

Source capture currently requires Git-backed jj. It preserves an explicit source/history snapshot without mutating source bookmarks or copying linked workspace pointers. Known credential-like tracked names are refused.

SQLite uses online backup, including committed WAL state. Each database is independently consistent; this is not an atomic application-wide checkpoint. File resources depend on your quiescence assertion. Capture does not stop their writers.

Use `--source-only` only when leaving every declared data resource uncaptured is intended. The receipt records that scope. It is not an application-ready replica.

## Restore without activation

```sh
parcela transfer restore "$CHECKPOINT" --destination "$NEW_REPLICA"
```

The destination must be absent. Existing output and restore paths are not overwritten, including paths created concurrently. Restored source and resources stay paused. Process identities, native handles, credentials and service activation are not transferred.

## Stream a SQLite snapshot explicitly

For the bounded SSH streaming protocol:

```sh
parcela transfer stream-sqlite --database "$DATABASE" --to "$DESTINATION" \
  --receiver "$TARGET_PARCELA" --output "$NEW_TARGET_SNAPSHOT"
```

Use matching source and receiver builds; protocol-version negotiation is not implemented. The receiver parent must exist, be user-owned and private, and the target file must be absent.

A lost final acknowledgment can leave a valid published snapshot. Inspect the target before retrying; do not overwrite it. The receipt describes a database snapshot, not complete environment transfer or active agent continuation.

See [supported scope](limits.md) before treating a copy as a runnable environment.
