# Changelog

## Unreleased

### Added

- `up --lan` and `up --tailscale` (and `restart`) also serve one environment to other devices: at `.local` names on the LAN through mDNS, or on the tailnet through `tailscale serve`. `PARCELA_SHARE_HOSTS` lists the shared hostnames for a dev server's host check.
- Colors on a terminal: help, errors, and a color per process in `parcela logs`. `NO_COLOR` turns them off.
- `hooks.teardown` runs on `parcela rm`, for example to drop the environment's database. A failed teardown keeps the environment registered.
- `install.sh` also installs Caddy and Process Compose into `~/.local/bin` when they are missing, checking each download against its release checksums.
- `parcela logs --follow` streams new lines from every process, or one.
- Nightly builds of `main`: `install.sh --nightly` installs the newest one. Builds other than a release tag report the commit in `--version`, like `0.3.0+20261019.4fed620`.
- The first `up` in a terminal trusts Parcela's certificate authority when it is not trusted yet, behind the system password prompt, once per authority. Never without a terminal; `PARCELA_NO_TRUST=1` turns it off.
- `parcela proxy install` on macOS makes URLs portless (`https://hello.localhost`): launchd owns 127.0.0.1:443 and relays each connection to the proxy as you. `uninstall` removes it.
- `http.wildcard: true` sends every subdomain of a service's host to it, for subdomain-per-tenant apps.
- `--lan-ip` picks the LAN address for `--lan`. `PARCELA_LAN=1` and `PARCELA_TAILSCALE=1` work like the flags.
- A JSON Schema for `parcela.yaml` at `https://parcela.davafons.com/schema/v1.json`, generated from the parser. `parcela init` adds the line editors use to find it.

### Changed

- The proxy listens on one HTTPS port, and URLs leave it out when it is 443. The forwarded "standard ports" mode and its port-80 listener are gone. `--port` accepts any port the system lets a normal user listen on.
- If the proxy dies while environments run, they start it again within a few seconds. `parcela proxy stop` still stops it for good.
- `restart` picks up added, removed and renamed services; `down` and `up` are no longer needed for that.
- `up` on a ready environment prints its URLs and exits 0, like `docker compose up -d`.
- Clearer errors: a readiness timeout says what each service answered, a busy `--backend-port` names its holder, and an unknown service or task lists the valid names.
- `--help` lists the global options in their own section. `status --all` is gone; use `ls`.

### Fixed

- Process Compose probes only services that another service or hook waits for, so health checks no longer fill every HTTP service's log.
- Errors that show `owner.log` lines no longer include Process Compose's terminal-title escape.

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
