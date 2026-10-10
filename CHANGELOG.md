# Changelog

## Unreleased

- Added explicit transfer policy and read-only transfer preflight. Missing/unsafe resources and unimplemented handoff capabilities remain blockers.
- Added private, checksummed jj checkpoints and paused independent restores, including linked workspaces and declared SQLite/file resources. Source-only capture reports uncaptured data and profiles.
- Added consistent SQLite snapshot streaming over SSH without full local staging. Existing destination files are preserved. Execution ownership, native agent activation, and Herdr layouts are not transferred by these commands.
