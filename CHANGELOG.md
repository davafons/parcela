# Changelog

## 0.3.0 - 2026-10-18

`parcela.yaml` has a new format, version 1. Files written for 0.2 or earlier need rewriting, and `parcela doctor` links to the new format. The [configuration reference](docs/configuration.md) shows the new format.

### Added

- `hooks.setup` runs once in each new environment and `hooks.prestart` on every `up`, after the services they `depends_on` and before every other service. A failed hook stops `up`. `up --setup` and `restart --setup` run setup again.
- Services take `depends_on`, `working_dir`, `env` and, without `http`, a `healthcheck` command. Dependencies wait for readiness.
- `PARCELA_DATA_DIR`, `PARCELA_ENVIRONMENT_NAME` and `PARCELA_PRIMARY_DIR`.
- Top-level `x-` keys are ignored, for YAML anchors.
- `examples/rails-postgres`, and a tutorial app with a SQLite database per environment.

### Changed

- One format: `version` and `project` are required, and every service is a mapping under `services` with a `command`. HTTP services declare `http: {healthcheck, subdomain}`; the one without `subdomain` gets the environment's URL.
- Tasks take a shell `command` instead of `argv`. Arguments after `--` are appended.
- `parcela logs` without a service prints every service, worker and hook, each line prefixed with its name.

### Removed

- The top-level short form (`web: bin/dev`), `workers`, `default_service`, `environment` (now `env`), `inherit_environment`, `shutdown_timeout_seconds` and the `process_compose` pass-through.

### Fixed

- `parcela doctor` names the right Process Compose tap (`f1bonacc1/tap/process-compose`) and links each backend's install page.

## 0.2.0 - 2026-10-18

A smaller CLI and a faster proxy.

### Changed

- `parcela status` lists each service and worker process; `parcela services` is removed.
- `parcela ls` also shows this project's jj workspaces and Git worktrees that were never started, with the URL they would get; the `workspace` commands are removed.
- `parcela doctor` reports the same configuration error as `up`; `parcela config check` is removed.
- `parcela up` takes back control of services left running after Parcela's supervising process died. `recover` and `context` are hidden.
- Foreground `up` prints `✓ <name> is running at <url>` and no longer shows Process Compose debug lines.
- `--human` is removed; text is the default on a terminal.

### Fixed

- Two simultaneous first `parcela up` runs no longer fail with "unsupported registry schema 0".
- The proxy refreshes routes in about 85 ms with five running environments, down from about 1.2 s, so routes no longer come close to their 3-second expiry under load.

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
