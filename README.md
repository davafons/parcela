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
| Primary checkout | `https://notes.localhost:1355` |
| Preview named `auth` | `https://auth-notes.localhost:1355` |

- **Named HTTPS.** Flat hostnames and exact-host routing through Caddy, on a no-sudo HTTPS port by default and on standard ports once host forwarding is set up.
- **Stable workspace identities.** UUIDs survive bookmark and branch changes. Startup registers the selected jj, Git or plain-directory checkout automatically.
- **Native service graphs.** Process Compose runs the web service and workers. Each environment gets a private control socket.
- **Explicit lifecycle control.** Start, stop, restart, inspect, and recover qualified native graphs without guessing which PID to kill.
- **Project-owned policy.** Your configuration defines commands, preparation, and data bindings. Core does not assume Rails, Node, or a database layout.
- **Readable or scriptable output.** Aligned text on a terminal, JSON when piped or with `--json`, and plain-text URLs and log tails.

Parcela is experimental. The tested source build is not a finished installer. See [project status](#project-status) for the current limits.

## Install

> **No release has been published yet.** The commands below take effect once the first `v*` tag is released. Until then, build from source.

With Homebrew on macOS or Linux, which also installs Caddy and Process Compose:

```sh
brew install davafons/tap/parcela
```

With the shell installer, which installs only the `parcela` binary into `~/.local/bin`:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/davafons/parcela/releases/latest/download/parcela-installer.sh | sh
```

Release builds target macOS (Apple silicon and Intel) and Linux (x86_64 and arm64, glibc). Each archive includes `LICENSE`, `COPYRIGHT` and `THIRD_PARTY_NOTICES.txt`. With the shell installer, install Caddy 2.x and Process Compose 1.x yourself. Parcela also needs `lsof` and `ps`.

From source, with Rust 1.89 or newer and a C toolchain for bundled SQLite:

```sh
cargo install --git https://github.com/davafons/parcela --locked
```

Neither installer starts a proxy, binds a port, or trusts a CA. Backends are never installed by an ordinary Parcela command.

### The HTTPS front door

`parcela up` starts Parcela's proxy in the background the first time it is needed. The proxy serves HTTPS on port 1355, so URLs look like `https://notes.localhost:1355`. It needs no sudo, port forwarding or trust change, and it keeps running for every project until `parcela proxy stop`.

To use another port, run `parcela proxy start --port 4443` once; the port is saved for later starts. If the port is busy, Parcela names the process holding it and stops. It never takes over a listener or falls back to another port.

Browsers warn about the certificate until you trust Parcela's local certificate authority. Run `parcela trust` once; it says what it will add, then asks for your password. `parcela trust --print` prints the certificate path for `curl --cacert` and changes nothing.

URLs without a port need ports 443 and 80 forwarded to the proxy. The macOS [operator helper](scripts/setup_macos.py) installs guarded loopback forwarding. Review the [host setup and verification notes](docs/implementation.md#real-machine-cutover-state) before using it.

## Quickstart

After building Parcela, create a `parcela.yaml` in your project and start it:

```sh
parcela init && parcela up
```

`init` guesses the dev server command from `package.json`, a Rails app or Django, and otherwise serves the directory with Python. The result is one line you can edit:

```yaml
web: python3 -m http.server $PORT --bind 127.0.0.1
```

The project is named after the checkout directory. Each service must listen on `127.0.0.1:$PORT`. The [configuration reference](docs/configuration.md) covers several services, workers and the full schema.

A project in a `notes` directory gets `https://notes.localhost:1355`. The first `up` also starts the proxy and says so on one line. `up` stays in the foreground by default; `--detach` returns once the services answer and keeps them running in the background.

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

For a named preview in another checkout:

```sh
parcela up --name review --detach
parcela url
```

The main checkout uses the project base URL. Linked jj workspaces use their workspace name; linked Git worktrees use their directory name. `--name` overrides the inferred preview name. The name is remembered after first startup, so branch or workspace renames do not rename a registered environment. `--primary` is an explicit override when a linked checkout is intended to own the base URL. Conflicts refuse rather than replacing another owner. No separate adoption command is required.

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

Current source passes static checks and actual backend acceptance on macOS and Linux, including crash, retry, stop, route-lease, and recovery cases. The [implementation notes](docs/implementation.md#lifecycle-recovery-verification-2026-10-13) record the tested revisions, results, and limits.

Still incomplete or unverified:

- Automatic reconciliation when launch evidence is missing.
- Concurrent startup and restart, immutable launch snapshots, stale successor cleanup, and reboot recovery.
- Interactive attach, following logs, per-service restart, sharing, aliases, and tenant routing.
- General data-isolation certification.
- Release packaging.

Parcela is not published to crates.io. Passing disposable tests does not prove that an installed host's listeners, forwarding, and trust are configured correctly.

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

- [Public documentation](https://parcela.davafons.com/docs/) covers the development build and its supported scope.
- [Task-first guides](docs/site/index.md) cover setup, the first environment, worktrees, service sets and agent use.
- [Build the documentation site](docs/documentation.md) describes the VitePress build, verification and deployment on every push to `main`.
- [Configuration and CLI](docs/configuration.md) contains the executable manifest and command contract.
- [Implementation status](docs/implementation.md) records verified behavior and remaining gaps.
- [Architecture](docs/architecture.md) describes ownership and backend boundaries.
- [PRD](docs/PRD.md) lists release goals, not shipped-feature claims.
