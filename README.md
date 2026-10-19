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
  <a href="https://parcela.davafons.com/docs/">Documentation</a> ·
  <a href="https://parcela.davafons.com/docs/getting-started/">Getting started</a> ·
  <a href="https://parcela.davafons.com/docs/cli/">CLI guide</a>
</p>

Parcela gives each checkout of your project its own web server, API, workers, ports and logs, at its own `https://*.localhost` URL. Your main checkout runs at `https://shop.localhost:1443`. The `signup` worktree runs at `https://signup-shop.localhost:1443`, at the same time.

## Install

```sh
curl -fsSL https://parcela.davafons.com/install.sh | sh
```

It also installs [Caddy](https://caddyserver.com) and [Process Compose](https://f1bonacc1.github.io/process-compose/) into `~/.local/bin` when they are missing. With Homebrew: `brew install davafons/tap/parcela`. Builds cover macOS and Linux; from source, run `cargo install --git https://github.com/davafons/parcela --locked`.

Releases are tagged, and `parcela --version` prints `0.3.0`. The newest build of `main` is the nightly, which adds the commit date and hash, like `0.3.0+20261019.4fed620`:

```sh
curl -fsSL https://parcela.davafons.com/install.sh | sh -s -- --nightly
```

## Quickstart

In your project:

```sh
parcela init      # write a parcela.yaml with your dev server command
parcela up        # start it at https://<project>.localhost:1443
parcela trust     # once, so browsers accept the certificate
```

`parcela doctor` checks the machine and the project and says how to fix anything missing.

## Parallel worktrees

Every jj workspace and Git worktree of a project is its own environment, so several agents can each run the full app at once:

```sh
git worktree add ../signup && cd ../signup
parcela up --detach
# ✓ signup (shop) is running at https://signup-shop.localhost:1443
```

Ports never collide. A `hooks.setup` command in `parcela.yaml` prepares each new worktree on its first `up`, for example by creating its own database. `parcela ls` lists every environment, `parcela down --all` stops them and `parcela rm --prune` forgets deleted checkouts. `parcela --skill` prints instructions for coding agents.

Parcela does not copy or isolate your data, install dependencies or run migrations. Your `parcela.yaml` decides what starts.

## Documentation

- [Getting started](docs/site/getting-started.md) builds a small app with an API, a worker and a second worktree.
- [Parallel worktrees and agents](docs/site/worktrees.md) covers naming, data separation and the Claude Code skill.
- [CLI guide](docs/site/cli.md) and [configuration reference](docs/configuration.md).
- [Architecture](docs/architecture.md) and [building the docs site](docs/documentation.md) are for contributors. Run `scripts/check` before sending a change.

## License

Copyright © 2026 David Afonso. Parcela is licensed under the [GNU Affero General Public License, version 3 only](LICENSE), with the project notice in [COPYRIGHT](COPYRIGHT).

The license permits commercial use and modification subject to its terms. It does not prohibit competing products. Distribution and network use of modified versions have source-sharing obligations. Third-party components retain their own licenses.
