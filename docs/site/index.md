---
layout: home
title: Deployable dev environments for your AI agents
titleTemplate: 'Parcela: :title'
description: Run multiple worktrees in parallel without conflicts. Each checkout runs the whole app at its own HTTPS URL, so parallel coding agents can each test a real running app.
search: false
hero:
  text: Deployable dev environments for your AI agents
  tagline: Run multiple agents in parallel without port conflicts.
  actions:
    - theme: brand
      text: Get started
      link: /docs/getting-started/
    - theme: alt
      text: GitHub
      link: https://github.com/lostcoords/parcela
---

## Run your project

### Configure the stack

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

Each HTTP service gets a free port in `$PORT` and its own URL. `hooks.setup` runs once in every new environment, so the setup command can install dependencies and prepare data for a fresh worktree.

### Start it

```sh
parcela up --detach
```

```text
✓ shop is running at https://shop.localhost:1443
```

### Open it

Visit `https://shop.localhost:1443`. `parcela logs` shows what every process printed, and `parcela down` stops it all.

[Get started](getting-started.md) with your app, or browse the [documentation](docs.md).
