# Architecture

Parcela is a Rust CLI. It supervises each environment's processes itself and uses Caddy as its proxy. It does not implement HTTPS or routing.

| Component | Owns |
|---|---|
| Parcela (Rust) | Configuration, environment identity, the registry, port and hostname reservations, lifecycle, and the process graph: start order, readiness, logs and shutdown |
| Caddy | HTTPS, certificates and routing, in a dedicated instance with its own config, storage and admin socket |
| The project | Commands, data, preparation and anything with external effects |

## Modules

- `config` parses and validates `parcela.yaml` (format version 1) into the runtime `Manifest`. Unknown keys are errors, except top-level `x-` keys.
- `registry` is the SQLite store of projects, environments, sessions and reservations.
- `startup` and `runtime` register a checkout, run its graph and wait for readiness. `ports` leases the backend ports.
- `supervisor` runs the graph: each process in its own process group, started once its dependencies are ready, stopped in reverse order. `logs` writes and reads each process's log file.
- `owner`, `control` and `lifecycle` run the per-environment owner process and the stop, restart and recover requests sent to it.
- `router`, `proxy` and `frontdoor` generate Caddy config, run the background proxy and pick its listen port.
- `share` plans `up --lan` and `up --tailscale`: the LAN address, the mDNS and `tailscale serve` publishers, and `PARCELA_SHARE_HOSTS`.
- `process` and `http` check listener ownership and readiness. `http` is a client, never a proxy.
- `discovery` reads jj and Git checkout inventory without changing it.
- `private` and `sockets` own private files, kernel locks and Unix socket paths.
- `src/cli` parses arguments and renders text or JSON.

## What `parcela up` does

1. Find the nearest `parcela.yaml` and validate it.
2. Register the checkout on first use. The main checkout gets the project URL. A linked jj workspace or Git worktree gets its own name.
3. Reserve every service hostname, then lease a loopback port from 20000–29999 for each service and named port. Leases are one transaction.
4. Start the proxy if it is not running.
5. Build the graph from the manifest: each HTTP service gets its port and an HTTP readiness check, hooks become one-shot processes after their dependencies, and every other service waits for the last hook. `setup` is in the graph only until it has succeeded once. The owner process holds `owner.lock`, serves `ctl.sock`, saves `owner.json` and then starts the processes: each runs with `bash -c` (`sh -c` without bash) in its own process group, writing to `log/processes/<name>.log`. A process starts once each dependency has started, answered its readiness check or, for a hook, exited 0. Sharing publishers are restarted up to 10 times when they fail; nothing else is restarted.
6. Wait until every HTTP service listens on its port, the listener belongs to the graph, and the readiness path answers 2xx.
7. Publish Caddy routes for the whole group. With `--detach`, return once the owner reports ready.

If the owner dies, its processes keep running. The next `up`, `down` or `recover` takes the ownership lock and adopts every recorded process whose birth identity (pid, start time and boot) still matches, then carries on from the graph's recorded state.

The proxy is `parcela proxy run` in the background, supervising one Caddy and renewing every running environment's routes twice a second. Routes expire 3 seconds after the last renewal, so a dead proxy never serves stale routes. If it dies while environments run, each owner restarts it within about 2 seconds and republishes its routes, unless `parcela proxy stop` left a `proxy/stopped` marker. Nothing starts the proxy at login or after a reboot; the next `up` does.

`up --lan` records the LAN address with the run. Caddy then adds a second server on that address and the HTTPS port, routing `<name>.local` to the run's services and nothing else, and only while an interface still has the address. If Caddy cannot listen there, the proxy loads the config without it, so loopback routes never go down with it. `up --tailscale` picks free tailnet HTTPS ports from `tailscale serve status`. The publishers, `dns-sd`/`avahi-publish` and a foreground `tailscale serve` per service, are processes in the run's graph, so they stop with it; Tailscale drops a foreground serve config when its process exits, even on SIGKILL.

`down` sends a stop request to that owner. The owner withdraws routes, sends SIGTERM to each process group once nothing that depends on it runs, then SIGKILL after 10 seconds, and marks the run stopped once no recorded process is alive. `restart` checks the replacement first and then starts a new generation.

## State layout

The state directory is `--state-dir`, then `PARCELA_STATE_DIR`, then `$XDG_STATE_HOME/parcela`, then `~/.local/state/parcela`. It is mode 700.

```text
<state>/
├── registry.sqlite3
├── proxy/                 caddy.json, frontdoor.json, proxy.log, data/ (CA and certificates)
└── environments/<uuid>/   context.json, owner.json, log/ (owner.log, processes/), hooks/, data/, tmp/, cache/
```

Control and Caddy admin sockets live in `<base>/<hash>`, where the hash comes from the state path. The base is `parcela` in macOS's per-user temporary directory, which another user cannot create first, and `/tmp/parcela-<uid>` on Linux. Neither depends on environment variables, so every process of a user agrees on it. Unix socket paths are limited to about 100 bytes.

## Safety rules

- Listen on loopback only, except for a run started with `--lan`, and serve the tailnet only for a run started with `--tailscale`. Never take over a busy port. Report its holder instead.
- Never signal a bare PID read from disk. A recorded process is signaled only after its birth identity is read again and still matches, and only by an owner holding `owner.lock`. Between that check and the signal, the pid could in theory be reused; nothing else is ever signaled.
- Trust a CA only through `parcela trust`, or the first `up` on a terminal behind the system's password prompt; never without a terminal. Install a system service only through `parcela proxy install`: a launchd socket on 127.0.0.1:443 that runs `nc` as the user. Never redirect ports with pf, which would hide another program's listener.
- Never delete source or data. `rm` forgets a registration and keeps the runtime directory.
- Read-only commands never register a checkout or start anything.
- Keep framework and data policy in `parcela.yaml`, not in core.

## Run the checks

```sh
scripts/check
```

This runs `cargo fmt --check`, Clippy with `-D warnings`, `cargo test` and the Python tests.

The live suites run a real Caddy from `PATH`. Each fixture gets its own `/tmp` state directory, test CA and ports in 30000-32767, below the OS ephemeral range and outside Parcela's 20000-29999 leases. They never change system trust:

```sh
scripts/check-live                       # core, service-sets, ports, docs-example, recovery
scripts/check-live recovery --case owner_crash
```

The runner builds once, runs the suites in order, writes one JSON report per suite to a temporary directory (or `--output DIR`) and exits non-zero if any fails. `--caddy` overrides the binary.

Documentation checks are in [docs/website/README.md](website/README.md).
