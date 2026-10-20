<p align="center">
  <img src="assets/brand/logo.svg" alt="Parcela picket mascot" width="150">
</p>

<h1 align="center">Parcela</h1>

<p align="center">
  <img src="https://img.shields.io/badge/status-experimental-e8b44f" alt="Status: experimental">
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/built_with-Rust-b45d3c?logo=rust&amp;logoColor=white" alt="Built with Rust"></a>
  <img src="https://img.shields.io/badge/platforms-macOS%20%7C%20Linux-57966f" alt="Platforms: macOS and Linux">
  <a href="https://github.com/davafons/parcela/actions/workflows/check.yml"><img src="https://github.com/davafons/parcela/actions/workflows/check.yml/badge.svg" alt="Rust checks"></a>
</p>

<p align="center">
  Deployable dev environments for your AI agents.
</p>

<p align="center">
  <a href="https://parcela.run/docs/">Documentation</a> ·
  <a href="https://parcela.run/docs/getting-started/">Getting started</a> ·
  <a href="https://parcela.run/docs/faq/">FAQ</a>
</p>

Parcela starts a checkout's whole dev stack, its web server, API and workers, on ports it picks, and serves it at an HTTPS URL named after the checkout. Every jj workspace and Git worktree gets its own copy, running at the same time, so several coding agents can each try their change against a real app.

```console
~/shop $ parcela up --detach
✓ shop is running at https://shop.localhost:1443

~/shop $ git worktree add ../signup && cd ../signup
~/signup $ parcela up --detach
✓ signup (shop) is running at https://signup-shop.localhost:1443
  api       https://api-signup-shop.localhost:1443

~/signup $ parcela ls
PROJECT  NAME     STATE  URL                                 PATH
shop     signup   ready  https://signup-shop.localhost:1443  /Users/you/signup
shop     primary  ready  https://shop.localhost:1443         /Users/you/shop
```

No Docker and no root. Parcela runs the processes itself and uses [Caddy](https://caddyserver.com) for HTTPS and routing.

## Install

```sh
curl -fsSL https://parcela.run/install.sh | sh
```

It also installs Caddy into `~/.local/bin` when it is missing, checking each download against its release checksums, and never uses `sudo`. With Homebrew: `brew install davafons/tap/parcela`. From source: `cargo install --git https://github.com/davafons/parcela --locked`. Builds cover macOS and Linux.

## Quickstart

In your project:

```sh
parcela init      # write a parcela.yaml with your dev server command
parcela up        # start it at https://<project>.localhost:1443
```

The first `up` in a terminal offers to trust Parcela's local certificate authority, behind the system password prompt, so browsers accept the URLs. `parcela doctor` checks the machine and the project and says how to fix anything missing.

A `parcela.yaml` for a Rails app with a worker:

```yaml
version: 1
project: shop

services:
  web:
    command: bin/rails server -p $PORT
    http:
      healthcheck: /up
  jobs:
    command: bin/jobs

env:
  DATABASE_URL: sqlite3:${PARCELA_DATA_DIR}/app.sqlite3

hooks:
  setup:
    command: bin/rails db:prepare
```

Each HTTP service gets a free port in `$PORT` and its own URL. `hooks.setup` runs once in each new environment, so a fresh worktree creates its own database on its first `up`.

## What it does

- **Names from checkouts.** The main checkout is `https://shop.localhost:1443`. The `signup` worktree is `https://signup-shop.localhost:1443`, and its `api` service `https://api-signup-shop.localhost:1443`.
- **Ports that never collide.** Each HTTP service and named port gets a loopback port from 20000–29999, leased across every project and checkout.
- **Readiness, then routing.** `up` waits until every HTTP service answers its health check, then routes its URL. `--detach` returns once it is ready.
- **Processes that stop.** `parcela down` stops everything the run started. `parcela ls` lists every environment, `down --all-projects` stops them all and `rm --prune` forgets deleted checkouts.
- **A CLI agents can drive.** Every command prints JSON when piped. `parcela --skill` prints instructions for coding agents.
- **Sharing on request.** `up --lan` serves one run to your phone over mDNS, and `up --tailscale` to your tailnet. Never by default.

Parcela does not copy or isolate your data, install dependencies or run migrations. Your `parcela.yaml` decides what starts. [Frameworks and data](docs/site/frameworks.md) shows per-worktree SQLite, PostgreSQL, Redis and uploads.

## How it compares

- **portless** names each dev server, and worktree branches too. **localias, puma-dev and Laravel Valet** name a port or a directory. Parcela adds a declared process graph per checkout: workers and databases next to the web servers, readiness checks before routing, a setup hook and a data directory for each new worktree, and one `down` for all of it.
- **Docker Compose with Traefik, devcontainers** isolate execution in containers. Parcela runs your native dev commands, so there is nothing to build or sync. A service can still be a container.
- **Conductor, Vibe Kanban and other agent managers** create worktrees and run agents. Parcela runs the app inside each worktree and works under any of them.

The [FAQ](docs/site/faq.md) answers the questions about certificates, port 1443, `.localhost`, Windows and uninstalling.

## Documentation

- [Getting started](docs/site/getting-started.md) builds a small app with an API, a worker and a second worktree.
- [Parallel worktrees and agents](docs/site/worktrees.md) covers naming, data separation and the Claude Code skill.
- [Frameworks and data](docs/site/frameworks.md) covers host checks, databases and uploads for Rails, Django, Vite, Next.js, Phoenix and Laravel.
- [CLI guide](docs/site/cli.md), [configuration reference](docs/site/configuration.md) and [troubleshooting](docs/site/troubleshooting.md).
- [Architecture](docs/architecture.md) and [building the docs site](docs/website/README.md) are for contributors. Run `scripts/check` before sending a change.

Releases are tagged, and `parcela --version` prints `0.4.0`. The newest build of `main` is the nightly, which adds the commit date and hash, like `0.4.0+20261020.4fed620`. `install.sh --nightly` installs it.

## License

Copyright © 2026 David Afonso. Parcela is licensed under the [GNU Affero General Public License, version 3 only](LICENSE), with the project notice in [COPYRIGHT](COPYRIGHT).

The license covers Parcela's own code. Running your project with Parcela puts no terms on your code. Distribution and network use of modified versions of Parcela have source-sharing obligations. Third-party components retain their own licenses.
