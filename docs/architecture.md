# Architecture

Parcela is a Rust CLI that coordinates two maintained programs. It does not implement a proxy or a process supervisor.

| Component | Owns |
|---|---|
| Parcela (Rust) | Configuration, environment identity, the registry, port and hostname reservations, lifecycle |
| Caddy | HTTPS, certificates and routing, in a dedicated instance with its own config, storage and admin socket |
| Process Compose | The process graph: commands, start order, readiness, logs and shutdown |
| The project | Commands, data, preparation and anything with external effects |

## Modules

- `config` parses and validates `parcela.yaml` (format version 1) into the runtime `Manifest`. Unknown keys are errors, except top-level `x-` keys.
- `registry` is the SQLite store of projects, environments, sessions and reservations.
- `startup` and `runtime` register a checkout, launch Process Compose and wait for readiness. `ports` leases the backend ports.
- `owner`, `control` and `lifecycle` run the per-environment owner process and the stop, restart and recover requests sent to it.
- `router`, `proxy` and `frontdoor` generate Caddy config, run the background proxy and pick its listen port.
- `process` and `http` check listener ownership and readiness. `http` is a client, never a proxy.
- `discovery` reads jj and Git checkout inventory without changing it.
- `private` and `sockets` own private files, kernel locks and Unix socket paths.
- `src/cli` parses arguments and renders text or JSON.

## What `parcela up` does

1. Find the nearest `parcela.yaml` and validate it.
2. Register the checkout on first use. The main checkout gets the project URL. A linked jj workspace or Git worktree gets its own name.
3. Reserve every service hostname, then lease a loopback port from 20000–29999 for each service and named port. Leases are one transaction.
4. Start the proxy if it is not running.
5. Generate the Process Compose graph from the manifest: each HTTP service gets its port and an HTTP readiness probe, hooks become one-shot processes after their dependencies, and every other service waits for the last hook. `setup` is in the graph only until it has succeeded once. Then start an owner process that holds `owner.lock` and serves `ctl.sock`.
6. Wait until every HTTP service listens on its port, the listener belongs to the graph, and the readiness path answers 2xx.
7. Publish Caddy routes for the whole group. With `--detach`, return once the owner reports ready.

`down` sends a stop request to that owner. The owner withdraws routes, stops the graph and marks the run stopped. `restart` checks the replacement first and then starts a new generation.

## State layout

The state directory is `--state-dir`, then `PARCELA_STATE_DIR`, then `$XDG_STATE_HOME/parcela`, then `~/.local/state/parcela`. It is mode 700.

```text
<state>/
├── registry.sqlite3
├── proxy/                 caddy.json, frontdoor.json, proxy.log, data/ (CA and certificates)
└── environments/<uuid>/   context.json, owner.json, process-compose.json, log/, data/, tmp/, cache/
```

Control, Process Compose and Caddy admin sockets live in `/tmp/parcela-<uid>/<hash>`, where the hash comes from the state path. Unix socket paths are limited to about 100 bytes.

## Safety rules

- Listen on loopback only. Never take over a busy port. Report its holder instead.
- Never signal a PID read from disk. Stop and recover need a live owner or matching process-birth evidence.
- Never trust a CA, change forwarding or install software, except through `parcela trust`.
- Never delete source or data. `rm` forgets a registration and keeps the runtime directory.
- Read-only commands never register a checkout or start anything.
- Keep framework and data policy in `parcela.yaml`, not in core.

## Run the checks

```sh
scripts/check
```

This runs `cargo fmt --check`, Clippy with `-D warnings`, `cargo test` and the Python tests.

The live suites run real Caddy and Process Compose from `PATH`. Each fixture gets its own `/tmp` state directory, test CA and ports in 30000-32767, below the OS ephemeral range and outside Parcela's 20000-29999 leases. They never change system trust:

```sh
scripts/check-live                       # core, service-sets, ports, docs-example, recovery
scripts/check-live recovery --case owner_crash
```

The runner builds once, runs the suites in order, writes one JSON report per suite to a temporary directory (or `--output DIR`) and exits non-zero if any fails. `--caddy` and `--process-compose` override the binaries.

Documentation checks are in [docs/documentation.md](documentation.md).
