<p align="center">
  <img src="logo.svg" alt="Parcela picket mascot" width="150">
</p>

<h1 align="center">Parcela</h1>

<p align="center">
  <img src="https://img.shields.io/badge/status-experimental-e8b44f" alt="Status: experimental">
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/built_with-Rust-b45d3c?logo=rust&amp;logoColor=white" alt="Built with Rust"></a>
  <img src="https://img.shields.io/badge/platforms-macOS%20%7C%20Linux-57966f" alt="Platforms: macOS and Linux">
  <a href="https://github.com/davafons/parcela/actions/workflows/check.yml"><img src="https://github.com/davafons/parcela/actions/workflows/check.yml/badge.svg" alt="Rust checks"></a>
</p>

<p align="center">
  Organize local workspaces and run their services at named HTTPS URLs.
</p>

<p align="center">
  <a href="#quickstart">Quickstart</a> ·
  <a href="#commands">Commands</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="#documentation">Documentation</a>
</p>

## Why Parcela?

Keep your primary checkout and previews identifiable, with their own service graphs and reserved URLs.

| Environment | URL |
| --- | --- |
| Primary checkout | `https://notes.localhost` |
| Preview named `auth` | `https://auth-notes.localhost` |

- **Named HTTPS.** Flat hostnames, standard ports, and exact-host routing through Caddy.
- **Stable workspace identities.** UUIDs survive bookmark and branch changes. Adopt existing jj, Git, or plain-directory checkouts.
- **Native service graphs.** Process Compose runs the web service and workers. Each environment gets a private control socket.
- **Explicit lifecycle control.** Start, stop, restart, inspect, and recover qualified native graphs without guessing which PID to kill.
- **Project-owned policy.** Your configuration defines commands, preparation, and data bindings. Core does not assume Rails, Node, or a database layout.
- **Scriptable output.** JSON by default, readable tables with `--human`, and plain-text URLs and log tails.

Parcela is experimental. The tested source build is not a finished installer or a complete environment-handoff system. See [project status](#project-status) for the current limits.

## Build from source

Requirements:

- Rust 1.89 or newer and a C toolchain for bundled SQLite.
- Caddy 2.x and Process Compose 1.x, available separately.
- `lsof` and `ps` for process and listener inspection.

From this checkout:

```sh
cargo build --release --locked
./target/release/parcela --help
```

To use the built command in the examples below, add it to your current shell's path:

```sh
export PATH="$PWD/target/release:$PATH"
```

This does not replace an installed binary or upgrade running services. Backends are never installed by an ordinary Parcela command.

### Set up the HTTPS front door

Normal URLs use ports 80 and 443. Listener ownership, forwarding where needed, and CA trust require explicit host setup before starting an application.

Parcela never takes over an occupied listener, silently falls back to a high port, or installs CA trust during startup. The macOS [operator helper](scripts/setup_macos.py) provides guarded loopback forwarding and explicit trust setup. Review the [host setup and verification notes](docs/implementation.md#real-machine-cutover-state) before using it.

High ports are available through an explicit test profile. They are not the normal no-port workflow.

## Quickstart

After building Parcela and completing host setup, add a `parcela.yaml` to your project. This minimal example runs Python's HTTP server and requires Python 3:

```yaml
version: 1
project:
  id: notes
  name: notes

default_service: web
services:
  web:
    readiness_path: /

process_compose:
  version: "0.5"
  processes:
    web:
      command: 'python3 -m http.server "${PORT}" --bind 127.0.0.1'
```

Validate the configuration and adopt the checkout explicitly:

```sh
parcela config check
parcela workspace adopt --role primary --label primary
```

If the dedicated front door is already running, check it with `parcela proxy status`. Otherwise, run it in a separate terminal with the profile selected during host setup.

For direct standard-port listeners:

```sh
parcela proxy run
```

For an explicitly configured macOS forwarding setup:

```sh
parcela proxy run --forwarded-standard-ports --http-port 8080 --https-port 8443
```

Then start the environment and inspect it:

```sh
parcela up --detach
parcela status --human
parcela services --human
parcela url
parcela logs web --tail 100
```

With the standard front door configured, this project's URL is `https://notes.localhost`. `up` stays in the foreground by default. `--detach` returns only after the live owner reports readiness.

When you finish:

```sh
parcela down
```

Stopping preserves source and data. Preparation, migrations, and resets are never an implicit part of `up`. The [configuration reference](docs/configuration.md) covers workers, tasks, environment bindings, and preview adoption.

## Commands

| Command | Purpose |
| --- | --- |
| `up [--detach]` | Start the current environment through Process Compose. |
| `run -- COMMAND...` | Run a literal command that consumes its assigned endpoint. |
| `down` or `stop` | Stop the selected owned environment and preserve its data. |
| `restart [--detach]` | Preflight a replacement, then restart the whole graph in a new generation. |
| `recover [--detach]` | Reattach to a proven surviving native graph without replaying services. |
| `status [--all] [--human]` | Inspect live ownership and health, separately from recorded intent. |
| `services [--human]` | Inspect the native service graph. |
| `logs [SERVICE] --tail N` | Print a bounded log tail. |
| `url` | Print the reserved URL, not a guarantee that it is reachable. |
| `task NAME -- ARGS` | Run a finite command defined by the project. |
| `workspace discover [--human]` | Discover sibling jj or Git checkouts without adopting or editing them. |
| `transfer` | Plan, capture, verify, or restore explicit paused checkpoints, not active execution handoff. |
| `doctor` | Report capabilities and optionally probe supplied backend versions. |
| `--skill` / `skill` | Print the bundled agent operating skill without project or registry access. |

Use `--path DIR` for an existing checkout or `--workspace UUID` for an adopted environment. Inspection does not start services. `url` and `logs` print plain text, while diagnostics go to stderr.

## Teach an agent

Print the operating skill from the binary the agent will use:

```sh
parcela --skill
```

`parcela skill` prints the same Markdown. Both commands work outside a configured checkout, without Caddy or Process Compose, and do not read or create registry state. Load the output into the agent's context. The source is [`.agents/skills/parcela/SKILL.md`](.agents/skills/parcela/SKILL.md).

The skill covers explicit environment selection, JSON health, detached startup, service URLs/logs, whole-graph restart/stop, qualified recovery and project tasks. It does not grant permission for setup, data changes, host configuration or starting another environment. Parcela does not install the skill into a global agent profile automatically.

## How it works

Parcela coordinates maintained backends rather than implementing another proxy or supervisor.

| Component | Owns |
| --- | --- |
| **Parcela, written in Rust** | Configuration, workspace identity, registry, endpoint ownership, and lifecycle coordination. |
| **Caddy** | HTTP routing, TLS, and the dedicated front door. |
| **Process Compose** | Service graphs, dependencies, supervision, and native shutdown. |
| **Your project** | Commands, writable resources, preparation, and external-effects policy. |

The registry is private and user-owned. State defaults to `PARCELA_STATE_DIR`, then `$XDG_STATE_HOME/parcela`, then `~/.local/state/parcela`. `--state-dir` overrides those locations. Detached owner output goes to the environment's private `log/owner.log`.

### Safety boundaries

- Loopback only by default. Tenant routing and sharing are off.
- Declared HTTP services share one environment and native graph. Put non-routed workers in that graph without declaring an HTTP route.
- Stop and recovery require qualified ownership evidence. A saved PID alone never authorizes signaling.
- Unknown launch evidence retains ownership instead of triggering cleanup or command replay.
- Private runtime paths do not automatically isolate your database, uploads, credentials, or external effects. The project must bind those resources explicitly.
- Existing or legacy services are not implicitly adopted by a newer build.

## Project status

Current source passes static checks and actual backend acceptance on macOS and Linux, including crash, retry, stop, route-lease, and recovery cases. The [implementation notes](docs/implementation.md#lifecycle-recovery-verification-2026-10-13) record the tested revisions, results, and limits.

Still incomplete or unverified:

- Automatic reconciliation when launch evidence is missing.
- Concurrent startup and restart, immutable launch snapshots, stale successor cleanup, and reboot recovery.
- Interactive attach, following logs, per-service restart, sharing, aliases, and tenant routing.
- Active agent continuation, complete Mac-to-Linux-to-Mac handoff, and general data-isolation certification.
- Release packaging and an independent review of the current lifecycle stack.

Parcela is not published to crates.io. A distribution license has not been selected. Passing disposable tests does not prove that an installed host's listeners, forwarding, and trust are configured correctly.

## Development and verification

```sh
scripts/check

scripts/check-live \
  --caddy /path/to/caddy \
  --process-compose /path/to/process-compose

scripts/check-recovery \
  --caddy /path/to/caddy \
  --compose /path/to/process-compose \
  --output /path/to/recovery-report.json
```

`scripts/check` runs formatting, Clippy, Rust tests, Python tests, and schema checks. Live and recovery checks use disposable source and state, explicit test ports, and a disposable CA without changing system trust.

The opt-in [Shirabe replica check](docs/check-portable-shirabe.md) covers paused destination acceptance. That result is not proof of active environment handoff.

## Documentation

- [Public documentation](https://parcela.davafons.com/docs/) covers the development build and its supported scope.
- [Task-first guides](docs/site/index.md) cover setup, the first environment, worktrees, service sets and agent use.
- [Build the documentation site](docs/documentation.md) describes the Starlight build, verification and explicit publication gate.
- [Configuration and CLI](docs/configuration.md) contains the executable manifest and command contract.
- [Implementation status](docs/implementation.md) records verified behavior and remaining gaps.
- [Architecture](docs/architecture.md) describes ownership and backend boundaries.
- [Transfer commands](docs/transfer.md) cover paused capture, integrity checks, and restore.
- [Portable environments](docs/portable-environments.md) defines the intended handoff contract.
- [PRD](docs/PRD.md) lists release goals, not shipped-feature claims.
