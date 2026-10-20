---
layout: home
title: Deployable dev environments for your AI agents
titleTemplate: 'Parcela: :title'
description: Run multiple worktrees in parallel without conflicts. Each checkout runs the whole app at its own HTTPS URL, so parallel coding agents can each test a real running app.
search: false
hero:
  text: Deployable dev environments for your AI agents
  tagline: Run multiple worktrees in parallel without conflicts.
  actions:
    - theme: brand
      text: Get started
      link: /docs/getting-started/
    - theme: alt
      text: GitHub
      link: https://github.com/davafons/parcela
---

## Install

```sh
curl -fsSL https://parcela.run/install.sh | sh
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
