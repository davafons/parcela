# Parcela

A service-agnostic workspace and local-environment organizer. Rust owns identity, configuration and lifecycle; managed external Caddy owns HTTP/TLS; Process Compose owns service graphs. Projects own commands and data policy.

Primary URLs are `https://shirabe.localhost`; previews are `https://auth-shirabe.localhost`. Flat names, tenants off, loopback only. Standard ports are the normal front door; high-port tests and external forwarding require explicit profiles, never silent fallback.

## Current implementation

- Strict `parcela.yaml`, stable UUID identities, explicit primary/preview adoption and a private SQLite registry.
- `proxy run/status`: dedicated Caddy, private Unix admin socket, exact-host routes and internal TLS. Automatic CA trust installation is disabled.
- `run -- ARGV`: literal foreground command, owned loopback endpoint, initial HTTP readiness, signal/exit handling and route teardown.
- `up`: foreground native Process Compose graph, private Unix control socket, one routed HTTP service plus workers.
- `task NAME -- ARGS`: explicit finite project commands, optional timeout, private result record.
- `status`: durable intent, not a live health guarantee. `doctor` optionally probes explicitly supplied backend versions.

The runtime is experimental. No detached application control, persisted-PID stop/recovery, automatic backend installation, sharing, tenant routing or general data-isolation certification. Inspection/adoption do not launch services or prove that a URL is live. A crashed session or unknown socket requires explicit reconciliation.

## Build and verify

Requires Rust 1.89+ and a C compiler for bundled SQLite; local checks use Rust 1.95.0 on macOS. Runtime needs separately installed Caddy 2.x, Process Compose 1.x and `lsof`/`ps`. A distribution license has not yet been selected.

```sh
cargo build --locked
scripts/check
scripts/check-live --caddy /path/to/caddy --process-compose /path/to/process-compose
scripts/check-shirabe --source /path/to/shirabe --caddy /path/to/caddy \
  --process-compose /path/to/process-compose
```

Live checks use disposable source/state and explicit test ports, without changing system trust. Shirabe checks use fresh schemas and a synthetic user, not a copied live dictionary. Linux CI configuration is not Linux service acceptance.

## Use an existing checkout

Create a project-owned manifest, then adopt explicitly:

```sh
parcela workspace adopt --role primary --label primary
parcela proxy run                 # separate foreground front door, prerequisites required
parcela up                       # or: parcela run -- your-command --literal-argument
parcela task test -- unit
parcela status
```

`run/up` do not perform preparation or install anything. Each command must consume its assigned endpoint and paths. Core context is routing-only until the project binds its resources. Application child output passes through; inspection commands print JSON and diagnostics use stderr.

State defaults to `PARCELA_STATE_DIR`, `$XDG_STATE_HOME/parcela`, then `~/.local/state/parcela`; `--state-dir` overrides them. Existing state must be private and user-owned. A short state path is needed for Unix socket limits.

## Explicit macOS standard-port setup

`scripts/setup_macos.py` is an operator-only helper, never called by normal commands. It configures a guarded loopback PF anchor/boot job and trusts the specified Parcela CA. Caddy remains unprivileged, with an explicitly configured `--forwarded-standard-ports --http-port 8080 --https-port 8443` profile. This preserves canonical port-free URLs and verifies the port-80 router marker before application startup. The helper requires an already running owned front door and root authorization; it refuses incompatible root rules or occupied listeners. Review its prerequisites before using it on another host.

## Documentation

- [PRD](docs/PRD.md): release goals, not implemented-feature claims.
- [Implementation status](docs/implementation.md): verification and remaining gaps.
- [Configuration and CLI](docs/configuration.md): executable contract.
- [Architecture](docs/architecture.md): ownership and backend boundaries.
- [Transfer commands](docs/transfer.md): paused snapshots, not active-work handoff.
- [Portable environments](docs/portable-environments.md): selected handoff contract.
