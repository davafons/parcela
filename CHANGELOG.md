# Changelog

## Unreleased

- Added owner-mediated `down`/`stop`, whole-graph `restart` and explicit `up --detach`. Detached startup verifies readiness and the retained owner identity; teardown preserves source/data and never signals saved PIDs.
- Added fresh live `status --all`, `services`, bounded `logs`, `url`, UUID targeting and readable `--human` inventory tables. Legacy/interrupted owners and malformed health replies remain unknown instead of claiming live health.
- Added read-only sibling jj/Git discovery with revisions, refs and adoption IDs, including branchless/detached checkouts. Discovery neither snapshots jj edits nor automatically adopts sources.

- Added a repeatable Linux acceptance check for paused full-data Shirabe replicas. It verifies named HTTPS, dictionary/tokenizer and upload access, disabled copied jobs, route teardown, and retained snapshot integrity without activating agents or transferring execution ownership. Tokenizer assets bind explicitly instead of depending on the source host's home directory.
- Fixed copied-queue validation to allow SQLite journal-mode/counter changes only after verified shutdown. Queue contents and other metadata must remain unchanged; retained inputs still require finalized, byte-identical snapshots. Failed commands retain their diagnostics, and the Rails probe works under non-UTF-8 locales.

- Added explicit transfer policy and read-only transfer preflight. Missing/unsafe resources and unimplemented handoff capabilities remain blockers.
- Added private, checksummed jj checkpoints and paused independent restores, including linked workspaces and declared SQLite/file resources. Source-only capture reports uncaptured data and profiles.
- Added consistent SQLite snapshot streaming over SSH without full local staging. Existing destination files are preserved. Execution ownership, native agent activation, and Herdr layouts are not transferred by these commands.
