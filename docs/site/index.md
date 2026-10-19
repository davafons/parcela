---
layout: home
title: Parcela
search: false
hero:
  text: Deployable dev environments for your AI agents
  tagline: Web, API and workers for each checkout, on HTTPS, side by side. Built so parallel coding agents can each test a real running app. No Docker.
  actions:
    - theme: brand
      text: Get started
      link: /getting-started/
    - theme: alt
      text: FAQ
      link: /faq/
    - theme: alt
      text: GitHub
      link: https://github.com/davafons/parcela
---

## Install

```sh
curl -fsSL https://parcela.davafons.com/install.sh | sh
```

Or with Homebrew: `brew install davafons/tap/parcela`. Builds cover macOS and Linux.

Using a coding agent? [Paste it this prompt](getting-started.md#let-your-agent-set-it-up) and it installs Parcela and configures your project.

## Five minutes to a running app

### Describe your app

`parcela init` writes a `parcela.yaml` for your dev server. Add your other processes and a setup command:

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

hooks:
  setup:
    command: bin/setup
```

Each HTTP service gets a free port in `$PORT` and its own URL. `hooks.setup` runs once in every new environment, so a fresh worktree installs its packages and creates its database on its own.

### Start it

```sh
parcela up --detach
```

```text
✓ shop is running at https://shop.localhost:1443
```

### Open it

Visit `https://shop.localhost:1443`. `parcela logs` shows what every process printed, and `parcela down` stops it all.

## What you get

**One command per checkout.** `parcela up` in any jj workspace or Git worktree starts that checkout's processes, waits until every HTTP service answers, and routes its URL to it: `https://<project>.localhost:1443` for the main checkout, `https://<checkout>-<project>.localhost:1443` for the others.

**No port bookkeeping.** Ports are leased from 20000–29999 and never collide, across projects and checkouts. Services find each other through variables like `PARCELA_SERVICE_API_URL`.

**Processes that stop.** Parcela supervises the graph itself, so `parcela down` stops everything a run started, including what an agent forgot.

**A CLI agents can drive.** Every command answers in JSON when piped, and `parcela --skill` prints instructions for coding agents.

**Your machine, unchanged.** Loopback only, no root, no port takeover. Trust is added only with your password, and only from a terminal.

## How it compares

- **portless** names each dev server, and worktree branches too. **localias, puma-dev and Laravel Valet** name a port or a directory. Parcela adds a declared process graph per checkout: workers and databases next to the web servers, readiness checks before routing, a setup hook and a data directory for each new worktree, and one `down` for all of it.
- **Docker Compose, devcontainers** isolate execution in containers. Parcela runs your native dev commands, with nothing to build or sync.
- **Conductor, Vibe Kanban and other agent managers** create worktrees and run agents. Parcela runs the app inside each one.

The [FAQ](faq.md) covers the other questions people ask first: certificates, `.localhost`, data, licensing and how to uninstall.
