---
layout: home
title: Parcela
search: false
hero:
  text: Deployable dev environments for your AI agents
  tagline: Every worktree runs the whole app at its own HTTPS URL, side by side.
  actions:
    - theme: brand
      text: Get started
      link: /getting-started/
    - theme: alt
      text: GitHub
      link: https://github.com/davafons/parcela
---

## Install

```sh
curl -fsSL https://parcela.davafons.com/install.sh | sh
```

Or with Homebrew: `brew install davafons/tap/parcela`.

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
  api       https://api-shop.localhost:1443
```

### Open it

Visit `https://shop.localhost:1443`. `parcela logs` shows what every process printed, and `parcela down` stops it all.

## Why agents need it

**A CLI they can drive.** Every command answers in JSON when piped, so an agent starts, checks and stops its own copy of your app.

**Parallel copies, no port conflicts.** Run `parcela up` in another worktree and it gets its own ports, logs and URL, like `https://signup-shop.localhost:1443`. Each agent verifies its change against a real running app.

[Build your first environment](getting-started.md).
