---
layout: home
title: Parcela
search: false
hero:
  name: Parcela
  text: An HTTPS URL for every checkout
  tagline: Run each worktree's web server, API and workers side by side, each at its own https://*.localhost address.
  image:
    src: /assets/logo.svg
    alt: Parcela
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
features:
  - title: One URL per checkout
    details: Your main checkout runs at hello.localhost. The signup worktree runs at signup-hello.localhost. No port numbers to remember.
  - title: The whole app in one command
    details: The web server, the API and the background workers start, restart and stop together.
  - title: Made for parallel agents
    details: Each jj workspace or Git worktree gets its own ports, logs and URLs, so several agents can run the full app at once.
  - title: Your commands, your data
    details: parcela.yaml holds your own commands. Parcela never installs dependencies, runs migrations or touches your data.
---

## Three commands

Install Parcela:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/davafons/parcela/releases/latest/download/parcela-installer.sh | sh
```

Then, in your project:

```sh
parcela init
parcela up
```

`init` writes a one-line `parcela.yaml` with your dev server command. `up` starts it and prints its URL. For a project in a directory called `hello`, that is `https://hello.localhost:1443`.

## What you get

- A named HTTPS URL for each service: `https://hello.localhost:1443` and `https://api-hello.localhost:1443`.
- Free ports picked for you. Two checkouts of the same project never collide.
- Web servers, APIs and workers in one process group, with `parcela logs`, `restart` and `down`.
- A list of every running environment on the machine with `parcela ls`.
- Plain text in a terminal and JSON for scripts and agents.

## What Parcela does not do

- It does not copy or isolate databases. You point each checkout at its own data. The [worktrees guide](worktrees.md#keep-data-separate) shows how.
- It does not install your dependencies or run migrations, seeds or other setup.
- It does not trust its certificate until you run `parcela trust`.
- It listens on `127.0.0.1` only. Nothing is shared on the LAN.
- It serves URLs with a port (`:1443`). URLs on port 443 without a port number are not supported yet.

Parcela runs on macOS and Linux. It drives [Caddy](https://caddyserver.com) for HTTPS and [Process Compose](https://f1bonacc1.github.io/process-compose/) for processes.

[Start the tutorial](getting-started.md).
