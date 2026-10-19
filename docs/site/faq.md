# FAQ

## What does Parcela change on my machine?

- **Files.** State in `~/.local/state/parcela`, or `$XDG_STATE_HOME/parcela` (mode 700), and sockets in a per-user directory: `$(getconf DARWIN_USER_TEMP_DIR)parcela` on macOS, `/tmp/parcela-<uid>` on Linux. Nothing in your checkout except the `parcela.yaml` that `parcela init` writes.
- **Binaries.** `install.sh` puts `parcela`, and Caddy when it is missing, in `~/.local/bin`. It never uses `sudo`.
- **Listeners.** One Caddy on `127.0.0.1:1443`, and your services on loopback ports from 20000–29999. Nothing listens on your network unless you run `up --lan` or `up --tailscale`, and only for that run.
- **Trust.** Parcela's certificate authority is added to the system trust store only by `parcela trust` or the first `up` in a terminal, behind the system password prompt. Scripts, agents and CI never change trust.
- **System services.** None, unless you run `parcela proxy install` on macOS for URLs without a port.

Parcela sends nothing over the network. Sharing with `--lan` publishes mDNS names, and `--tailscale` runs `tailscale serve`, both until the run stops.

## Is trusting the certificate authority safe?

It is the same trade-off as [mkcert](https://github.com/FiloSottile/mkcert): a local root that browsers trust, so `https://*.localhost` works without warnings.

- The authority is Caddy's local CA, generated on your machine. Its private key is in `~/.local/state/parcela/proxy/data/pki/authorities/local/root.key`, readable only by your user.
- It has no name constraints. Whoever can read that key can issue a certificate your browser accepts for any site. Do not sync or back up `~/.local/state/parcela` to places other people can read, and keep it out of dotfiles repositories.
- A program that can read files as your user can already do more harm than this, for example by reading your browser profile.

`parcela trust` prints the exact undo command before it changes anything:

```sh
caddy untrust --cert "$(parcela trust --print)"
```

To never be asked, set `PARCELA_NO_TRUST=1`. Browsers then warn on every URL, and tools need `--cacert "$(parcela trust --print)"`.

## Why port 1443?

A normal user can listen on it without `sudo`, and Parcela never takes a port another program holds. For URLs without a port:

- **macOS:** `parcela proxy install` adds a launchd service that owns `127.0.0.1:443` and relays each connection to the proxy as you. No Parcela code runs as root. `parcela proxy uninstall` removes it.
- **Linux:** `sudo sysctl net.ipv4.ip_unprivileged_port_start=443`, then `parcela proxy start --port 443`.

## Do `*.localhost` names work everywhere?

Browsers resolve every `*.localhost` name to loopback themselves, as [RFC 6761](https://www.rfc-editor.org/rfc/rfc6761#section-6.3) asks. So does curl. Other clients use the system resolver: macOS resolves them, and on Linux it depends on systemd-resolved or `nss-myhostname`. [Frameworks and data](frameworks.md) has the details, and how to make Node, Python, Ruby and Go trust the certificate.

In Safari, type the scheme, `https://shop.localhost:1443`, or Safari may search for the name instead.

## How is it different from portless, localias or puma-dev?

[portless](https://github.com/vercel-labs/portless) names each dev server, and puts a worktree's branch in front. localias and puma-dev name a port or a directory. Parcela runs what a checkout's `parcela.yaml` declares: web servers, workers and databases, each HTTP service behind a readiness check before it is routed, with leased ports, a setup hook and a private data directory for each new worktree. `parcela down` stops all of it.

## Why not Docker Compose, devcontainers or a container per agent?

Use them if you need isolated execution or exact production images. Parcela is for the common case where your app already runs natively with `bin/dev` or `npm run dev`: it adds no image builds, no file sync and no VM on macOS. Each process is a normal process of your user, so debuggers, file watchers and your editor work unchanged.

A service can still be a container, like PostgreSQL per environment with its files in the environment's data directory:

```yaml
ports: [db]
services:
  db:
    command: >-
      docker run --rm -e POSTGRES_HOST_AUTH_METHOD=trust -p 127.0.0.1:$PARCELA_PORT_DB:5432
      -v "$PARCELA_DATA_DIR/postgres:/var/lib/postgresql/data" postgres:17
    healthcheck: pg_isready -h 127.0.0.1 -p "$PARCELA_PORT_DB"
```

## How does it relate to Conductor, Vibe Kanban, Claude Code and other agent tools?

Those manage agents and worktrees. Parcela runs the app inside each worktree, so it works under any of them, or with plain `git worktree add`. `parcela --skill` prints instructions that teach an agent to start, check and stop its own environment.

## Does each worktree get its own database?

Only if `parcela.yaml` says so. Parcela separates ports, URLs, processes and logs, and gives each environment a private `PARCELA_DATA_DIR`. Two checkouts that read the same `.env` share the same database. [Keep data separate](worktrees.md#keep-data-separate) and [Frameworks and data](frameworks.md) show SQLite, PostgreSQL, Redis and file uploads per worktree.

## Does it work on Windows or WSL?

Windows is not supported. Parcela uses Unix sockets, process groups and `flock`. WSL 2 runs Linux binaries, but it is untested: a browser on Windows would need to reach the WSL loopback and trust the authority itself.

## Why AGPL?

So improvements to Parcela stay open, including in hosted versions of it. It covers Parcela's own code. Your app only runs under Parcela, like under a shell, so the license puts no terms on your code. There is no contributor license agreement. See [COPYRIGHT](https://github.com/davafons/parcela/blob/main/COPYRIGHT) for the exact terms.

## Is the configuration format stable?

`parcela.yaml` has `version: 1`. Parcela is experimental, so it may still change, and every change is in the [changelog](https://github.com/davafons/parcela/blob/main/CHANGELOG.md). A file without `version: 1` is refused, never misread, and the error says which version this Parcela reads.

## How do I uninstall it?

```sh
parcela down --all-projects                    # stop every environment
parcela rm                                     # in each checkout whose hooks.teardown drops outside data
caddy untrust --cert "$(parcela trust --print)" # if you trusted the authority
parcela proxy uninstall                        # macOS, if you ran proxy install
parcela proxy stop
rm -rf ~/.local/state/parcela                  # all state, including environment data
rm -rf "$(getconf DARWIN_USER_TEMP_DIR)parcela"  # macOS sockets; /tmp/parcela-$(id -u) on Linux
rm ~/.local/bin/parcela
```

Remove `~/.local/bin/caddy` too if `install.sh` installed it and nothing else uses it. With Homebrew, run `brew uninstall parcela`. Parcela writes nothing into your checkouts except the `parcela.yaml` from `init`. Data your hooks put elsewhere, such as databases on a shared server, stays until you drop it.
