# Changelog

## Unreleased

### Removed

- Removed workspace transfer, checkpoints, SQLite streaming and the portable Shirabe replica check, including the `transfer` command and the `publish_directory` helper. Parcela now maintains local parallel environments only. The removed implementation is preserved on the jj bookmark `archive/transfer`. Manifests with a `transfer:` key are now rejected as unknown fields.

### Added

- Added owner-mediated `down`/`stop`, whole-graph `restart` and explicit `up --detach`. Detached startup verifies readiness and the retained owner identity; teardown preserves source/data and never signals saved PIDs.
- Added fresh live `status --all`, `services`, bounded `logs`, `url`, UUID targeting and readable `--human` inventory tables. Legacy/interrupted owners and malformed health replies remain unknown instead of claiming live health.
- Added read-only sibling jj/Git discovery with revisions, refs and adoption IDs, including branchless/detached checkouts. Discovery neither snapshots jj edits nor automatically adopts sources.

