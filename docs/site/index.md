---
layout: home
title: Parcela
search: false
hero:
  name: Parcela
  text: Deployable dev environments for your AI agents
  tagline: Every worktree runs the whole app at its own HTTPS URL, side by side.
  actions:
    - theme: brand
      text: Get started
      link: /getting-started/
    - theme: alt
      text: CLI guide
      link: /cli/
    - theme: alt
      text: GitHub
      link: https://github.com/davafons/parcela
---

## Install

```sh
curl -fsSL https://parcela.davafons.com/install.sh | sh
```

Or with Homebrew: `brew install davafons/tap/parcela`.

## A CLI your agent can drive

```sh
parcela init
parcela up --detach
```

`init` writes a one-line `parcela.yaml`. `up` starts your web server, API and workers and prints the URL. Every command answers in JSON when piped, so an agent can start, check and stop its own copy of your dev environment.

## Parallel environments, no port conflicts

Each worktree gets its own ports, logs and URL. Run one copy per agent, and each one verifies its change against a real running app.

[Build your first environment](getting-started.md).
