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
| Primary checkout | `https://notes.localhost:1443` |
| Preview named `auth` | `https://auth-notes.localhost:1443` |

- **Named HTTPS.** Flat hostnames and exact-host routing through Caddy, on a no-sudo HTTPS port by default and on standard ports once host forwarding is set up.
- **Stable workspace identities.** UUIDs survive bookmark and branch changes. Startup registers the selected jj, Git or plain-directory checkout automatically.
- **Native service graphs.** Process Compose runs the web service and workers. Each environment gets a private control socket.
- **Explicit lifecycle control.** Start, stop, restart, inspect, and recover qualified native graphs without guessing which PID to kill.
- **Project-owned policy.** Your configuration defines commands, preparation, and data bindings. Core does not assume Rails, Node, or a database layout.
- **Readable or scriptable output.** Aligned text on a terminal, JSON when piped or with `--json`, and plain-text URLs and log tails.

Parcela is experimental. See [project status](#project-status) for the current limits.

## Install

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/davafons/parcela/releases/latest/download/parcela-installer.sh | sh
```

The installer puts `parcela` in `~/.local/bin`. Builds cover macOS (Apple silicon and Intel) and Linux (x86_64 and arm64, glibc 2.35+). Each archive includes `LICENSE`, `COPYRIGHT` and `THIRD_PARTY_NOTICES.txt`.

Parcela drives [Caddy](https://caddyserver.com/docs/install) 2.x and [Process Compose](https://f1bonacc1.github.io/process-compose/installation/) 1.x. Install them once:

```sh
# macOS
brew install caddy f1bonacc1/tap/process-compose

# Linux: Caddy from your distribution or caddyserver.com, then Process Compose
sh -c "$(curl -fsSL https://raw.githubusercontent.com/F1bonacc1/process-compose/main/scripts/get-pc.sh)" -- -b ~/.local/bin
```

`lsof` and `ps` must be on `PATH`. `parcela doctor` checks all of it and names the fix for anything missing.

From source, with Rust 1.89 or newer and a C toolchain:

```sh
cargo install --git https://github.com/davafons/parcela --locked
```

### The HTTPS front door

`parcela up` starts Parcela's proxy in the background the first time it is needed. The proxy serves HTTPS on port 1443, so URLs look like `https://notes.localhost:1443`. It needs no sudo, port forwarding or trust change, and it keeps running for every project until `parcela proxy stop`.

To use another port, run `parcela proxy start --port 4443` once; the port is saved for later starts. If the port is busy, Parcela names the process holding it and stops. It never takes over a listener or falls back to another port.

Browsers warn about the certificate until you trust Parcela's local certificate authority. Run `parcela trust` once; it says what it will add, then asks for your password. `parcela trust --print` prints the certificate path for `curl --cacert` and changes nothing.

URLs without a port (standard ports 443 and 80) are not supported in this release.

## Quickstart

Create a `parcela.yaml` in your project and start it:

```sh
parcela init && parcela up
```

`init` guesses the dev server command from `package.json`, a Rails app or Django, and otherwise serves the directory with Python. The result is one line you can edit:

```yaml
web: python3 -m http.server $PORT --bind 127.0.0.1
```

The project is named after the checkout directory. Each service must listen on `127.0.0.1:$PORT`. The [configuration reference](docs/configuration.md) covers several services, workers and the full schema.

A project in a `notes` directory gets `https://notes.localhost:1443`. The first `up` also starts the proxy and says so on one line. `up` stays in the foreground by default; `--detach` returns once the services answer and keeps them running in the background.

If something is missing, `parcela doctor` checks the machine and the project. Every ✗ line names its fix.

Inspect a running environment:

```sh
parcela status
parcela services
parcela url
parcela logs web --tail 100
```

When you finish:

```sh
parcela down
```

Stopping preserves source and data. Preparation, migrations, and resets are never an implicit part of `up`. The [configuration reference](docs/configuration.md) covers workers, optional tasks, environment bindings and preview naming.

## Parallel environments for agents

Every checkout of a project gets its own environment: its own services, ports, logs and URL. Several agents can each work in their own jj workspace or Git worktree and run the full app at the same time.

```sh
jj workspace add ../notes-auth        # or: git worktree add ../notes-auth
cd ../notes-auth && parcela up --detach
# notes-auth is running at https://notes-auth-notes.localhost:1443
```

The main checkout uses the project URL. Linked jj workspaces take their workspace name and Git worktrees their directory name; `--name review` overrides it. The name is remembered, so renaming a branch or workspace keeps the same URL. A name already taken by another checkout is refused, never replaced.

Backend ports come from 20000–29999 and are distinct for every running environment. For databases, Redis or an HMR socket, declare `ports: [db, redis]` and use `${PARCELA_PORT_DB}` in commands; each environment gets its own, kept across restarts. Parcela does not copy or isolate data for you: point each environment's database at its own port or file.

`parcela ls` lists every environment across projects, `parcela down --all` stops a project's environments and `parcela rm --prune` forgets checkouts that were deleted. Print the bundled agent instructions with `parcela --skill`.

A project file defines what starts, not how to install its dependencies or prepare data. Keep using the project's existing test/build commands. `parcela task` is an optional wrapper for commands that need its environment context or timeout, not a prerequisite for `up`.

## Commands

| Command | Purpose |
| --- | --- |
| `init [--name NAME] [--command CMD]` | Write a minimal `parcela.yaml`, guessing the dev server command. Never overwrites. |
| `up [--detach] [--name NAME]` | Start the current environment through Process Compose. |
| `down [--all \| --all-projects]` or `stop` | Stop the selected environment, or every running one, and keep its data. |
| `restart [--detach]` | Check the replacement, then restart every service. |
| `status [--all]` | Show live health, separately from what was last recorded. |
| `ls` | List every registered environment across projects. |
| `services` | Show each service and worker process. |
| `logs [SERVICE] --tail N` | Print a bounded log tail. |
| `url [--service NAME]` | Print the reserved URL, not a guarantee that it is reachable. |
| `rm [--prune]` | Forget a stopped environment and release its hostnames. Source and data stay on disk. |
| `doctor` | Check tools, configuration, state and the proxy, with a fix for each failure. |
| `proxy start [--port N]`, `stop`, `restart`, `status` | Manage the background HTTPS proxy. `up` starts it when needed. |
| `trust [--print]` | Trust Parcela's local certificate authority, or print its certificate path. |
| `config check` | Validate `parcela.yaml` and that every service has a command. |
| `run -- COMMAND...` | Run a literal command that consumes its assigned endpoint. |
| `recover [--detach]` | Take back control of services that kept running after an interruption, without restarting them. |
| `task NAME -- ARGS` | Run a finite command defined by the project. |
| `workspace discover` | Discover sibling jj or Git checkouts without registering or editing them. |
| `--skill` / `skill` | Print the bundled agent operating skill without project or registry access. |

`parcela` alone shows the current checkout's status, or the next command to run. Output is aligned text on a terminal and JSON when stdout is a pipe; `--json` forces JSON. Use `--path DIR` for an existing checkout or `--workspace UUID` for a registered environment. Inspection does not start services. `url` and `logs` print plain text, while diagnostics go to stderr.

## Teach an agent

Print the operating skill from the binary the agent will use:

```sh
parcela --skill
```

`parcela skill` prints the same Markdown. Both commands work outside a configured checkout, without Caddy or Process Compose, and do not read or create registry state. Load the output into the agent's context. The source is [`.agents/skills/parcela/SKILL.md`](.agents/skills/parcela/SKILL.md).

The skill covers explicit environment selection, JSON health, detached startup, service URLs/logs, whole-graph restart/stop, qualified recovery and project tasks. It does not grant permission for setup, data changes, host configuration or starting another environment. Parcela does not install the skill into a global agent profile automatically.

The [agent guide](docs/site/agents.md#install-the-skill-for-claude-code) has a copyable Claude Code skill installation command and a [setup request](docs/site/agents.md#tldr-ask-your-agent-to-configure-this-checkout) for your existing agent session.

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

Version 0.1.0 passes static checks and live backend acceptance on macOS with real Caddy and Process Compose: parallel environments, the background proxy, named ports, crash, stop, restart and recovery cases. Linux builds are produced but this release's live suites ran only on macOS.

Not supported yet:

- URLs on standard ports 443 and 80 without a port number.
- Interactive attach, following logs, per-service restart, sharing, aliases and tenant routing.
- Automatic data isolation. Each project binds its own databases and files per environment.

## License

Copyright © 2026 David Afonso. Parcela is licensed under the [GNU Affero General Public License, version 3 only](LICENSE), with the project notice in [COPYRIGHT](COPYRIGHT).

The license permits commercial use and modification subject to its terms. It does not prohibit competing products. Distribution and network use of modified versions have source-sharing obligations. Third-party components retain their own licenses.

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

## Documentation

- [Public documentation](https://parcela.davafons.com/docs/) covers installation, the first environment, worktrees and agents.
- [Task-first guides](docs/site/index.md) cover setup, the first environment, worktrees, service sets and agent use.
- [Build the documentation site](docs/documentation.md) describes the VitePress build, verification and deployment on every push to `main`.
- [Configuration and CLI](docs/configuration.md) contains the executable manifest and command contract.
- [Implementation status](docs/implementation.md) records verified behavior and remaining gaps.
- [Architecture](docs/architecture.md) describes ownership and backend boundaries.
- [PRD](docs/PRD.md) lists release goals, not shipped-feature claims.
