# Changelog

## Unreleased

### Changed

- `parcela status` lists each service and worker process; `parcela services` is removed.
- `parcela ls` also shows this project's jj workspaces and Git worktrees that were never started, with the URL they would get; the `workspace` commands are removed.
- `parcela doctor` reports the same configuration error as `up`; `parcela config check` is removed.
- `parcela up` takes back control of services left running after Parcela's supervising process died. `recover` and `context` are hidden.
- Foreground `up` prints `✓ <name> is running at <url>` and no longer shows Process Compose debug lines.
- `--human` is removed; text is the default on a terminal.

### Fixed

- Two simultaneous first `parcela up` runs no longer fail with "unsupported registry schema 0".

### Removed

- `scripts/setup_macos.py` and the Shirabe acceptance harness.

## 0.1.0 - 2026-10-17

First release. Run every checkout of a project as its own local environment, at its own HTTPS URL, in parallel.

### Added

- `parcela init` writes a one-line `parcela.yaml`, guessing the dev server command from `package.json`, Rails or Django.
- Short-form manifests: `web: <command>`, a `services:` map of commands, `workers:` for unrouted processes and `ports:` for named non-HTTP ports. The full schema still works.
- `parcela up` registers the checkout itself. The main checkout gets the project URL; linked jj workspaces and Git worktrees get their workspace or directory name, or `--name`.
- A background HTTPS proxy on the no-sudo port 1443, started by the first `up`. `parcela proxy start|stop|restart|status` manage it, and `--port` saves another port.
- `parcela trust` installs Parcela's local CA after saying what it changes; `--print` only prints the certificate path.
- Named ports (`ports: [db, redis]`) export `PARCELA_PORT_<NAME>`, distinct per environment and kept across restarts.
- `parcela ls`, `down --all`, `down --all-projects`, `rm` and `rm --prune`.
- `parcela doctor` checks backends, configuration, state and the proxy, and names the next step.
- Aligned text on a terminal and JSON when piped or with `--json`.
- Prebuilt binaries for macOS and Linux with a shell installer.

### Fixed

- Environments whose supervisor and services all died (for example after `kill -9`) no longer block `up` and `down`.
- Services listening on `localhost`, `[::1]` or a wildcard address are accepted when they belong to the environment.
- Unix sockets live in `/tmp/parcela-<uid>`, so long home or state paths work.
- Backend ports come from 20000–29999 instead of the OS ephemeral range, so other programs cannot take them before a service binds.

### Removed

- Workspace transfer, checkpoints and SQLite streaming. Manifests with a `transfer:` key are rejected.
- `--test-profile` and the raw proxy listener flags; a saved port profile replaces them.
