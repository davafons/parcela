# Configuration and CLI

## Short form

Most projects need one line. `parcela init` writes it for you.

```yaml
web: npm run dev -- --port $PORT --host 127.0.0.1
```

Each top-level `name: command` is an HTTP service and a Process Compose process with that command. Several services and background workers look like this:

```yaml
services:
  web: bin/rails server -p $PORT -b 127.0.0.1
  api: node api/server.js --port $PORT
workers:
  jobs: bin/jobs
```

Conventions fill in the rest:

- `version` defaults to 1.
- The project is named after the main checkout directory, sanitized to a DNS label. Every jj workspace and Git worktree of a repository resolves to the same main checkout, so they share one project. Override it with `project: my-app` or `project: {id: ..., name: ...}`.
- The default service, which gets the base URL, is `web` when present, otherwise the only service. With several services and no `web`, set `default_service`.
- Every service gets its own `PORT`. In a non-default service's command, `$PORT` and `${PORT}` are rewritten to that service's `${PARCELA_SERVICE_<NAME>_PORT}`, because Process Compose expands braced variables from the shared environment when it loads the graph.
- `workers` are processes without a port or a route.
- A service can be a mapping when it needs more: `api: {command: node api.js, subdomain: backend, readiness_path: /up}`.

`ports`, `tasks`, `environment`, `inherit_environment` and `shutdown_timeout_seconds` work in both forms. The short form generates `process_compose`, so it refuses a hand-written `process_compose` block. Declare services either at the top level or under `services:`, not both. A top-level key that is close to a field name, such as `defualt_service`, fails with a suggestion instead of becoming a service.

## Named ports

Databases, Redis, HMR websockets, debuggers and mail catchers need ports too, and two worktrees running the same project must not share them. List their names under `ports` and Parcela leases a free loopback port for each one on every run:

```yaml
web: npm run dev -- --port $PORT --host 127.0.0.1 --strictPort
workers:
  db: postgres -D .data/pg -k /tmp -h 127.0.0.1 -p ${PARCELA_PORT_DB}
  redis: redis-server --port ${PARCELA_PORT_REDIS}
ports: [db, redis, vite-hmr]
environment:
  DATABASE_URL: postgres://127.0.0.1:${PARCELA_PORT_DB}/app
```

- Each name is a DNS label. Its port arrives as `PARCELA_PORT_<NAME>`, uppercased with hyphens turned into underscores, so `vite-hmr` becomes `PARCELA_PORT_VITE_HMR`. Every process, every task, `context.json` and `environment` values receive it.
- Write `${PARCELA_PORT_DB}` in a command. Process Compose expands it when it loads the graph, and the process environment carries the same value, so `$PARCELA_PORT_DB` in a shell command resolves to it as well. Process Compose 1.122 was observed expanding both.
- A Vite HMR socket reads its port from the config, for example `server: {hmr: {port: Number(process.env.PARCELA_PORT_VITE_HMR)}}`. Keep `--strictPort` on the dev server so Vite exits instead of drifting off its leased `PORT`.
- Named ports are leased with the service endpoints in one transaction. No other running environment, endpoint or named port can hold the same number.
- A new run reuses the environment's previous port for each name when it is still free, so a database URL survives `down`, `up` and `restart`. A port taken in the meantime is replaced by a fresh one, and the rest stay.
- Parcela does not route, probe or check ownership of these ports. The program may never listen on one. `parcela status` and `parcela context` list them.

## Full form

Commands locate the nearest `parcela.yaml` at or above `--path`. The manifest directory is the checkout root. `--workspace UUID` selects a registered environment instead of `--path`; the two options conflict. Explicit UUID inspection and stop still work when the source directory is missing. Starting or restarting always requires the current valid manifest. Use the same project identity across checkouts; startup registers the selected checkout with a stable role and name.

```yaml
version: 1
project: {id: my-app, name: my-app}
default_service: web
services:
  web:
    readiness_path: /up
inherit_environment: false
environment:
  APP_DATA: "${PARCELA_RUNTIME_DIR}/data"
tasks:
  test:
    argv: [bin/test]
    timeout_seconds: 300
process_compose:
  version: "0.5"
  processes:
    web:
      command: "your-server --bind 127.0.0.1"
    worker:
      command: "your-worker"
```

Project IDs are 1–128 ASCII letters/digits/hyphens/underscores/dots. DNS labels are 1–63 lowercase ASCII letters/digits with internal hyphens. Preview label plus project name must fit one label. Unknown organizer fields, invalid paths/names, NUL argv and nonpositive task timeouts fail. Native `process_compose` data is preserved, not interpreted as organizer fields. `config check` does not certify backend graph compatibility or resource isolation.

`up` runs all declared HTTP services in one worktree environment and one native graph. `run` remains a literal single-HTTP-service command. Non-HTTP workers belong in the native graph but not in `services`. In a service set, each routed service must name a graph process. Legacy single-service graphs can retain a different native process name. Services must bind their assigned ports only on `127.0.0.1`; HTTP answers from foreign listeners cannot establish readiness.

## Start from the project file

`parcela up` starts the native graph in this file. No separate `workspace adopt` or task command is required. `parcela up --name review` creates a named preview on first use. Main jj/Git checkouts and plain directories use the project base URL; linked jj workspaces use their workspace name, and linked Git worktrees use their directory name. Inferred names are DNS-normalized. Ambiguous or invalid names require an explicit `--name NAME` or `--primary` choice. `run` uses the same naming flags.

Existing identities are reused, not renamed when branches, directories or workspace metadata change. Explicit naming flags must match the registered identity. `--primary` explicitly selects the project base from a linked checkout, but cannot take over an existing primary. Registration and namespace conflicts fail without replacing owners. Read-only commands never register a checkout or start services.

`workspace adopt --role ... --label ...` remains an advanced explicit registration command. Project tasks are optional finite commands, not setup requirements. Startup does not install dependencies, prepare application data or repair the HTTPS front door.

## Service sets

The default service uses the environment base hostname. Other services use a flat prefix, supplied by `subdomain` or the service name. For project `my-app`, the primary frontend is `my-app.localhost` and the backend is `api-my-app.localhost`. A preview labeled `review` uses `review-my-app.localhost` and `api-review-my-app.localhost`.

```yaml
version: 1
project: {id: my-app, name: my-app}
default_service: frontend
services:
  frontend: {readiness_path: /up}
  backend: {subdomain: api, readiness_path: /up}
environment:
  API_ORIGIN: "${PARCELA_SERVICE_BACKEND_URL}"
process_compose:
  version: "0.5"
  processes:
    frontend:
      working_dir: frontend
      command: "./bin/dev --host ${HOST} --port ${PORT}"
    backend:
      working_dir: backend
      command: "./bin/dev"
      environment:
        - PORT=${PARCELA_SERVICE_BACKEND_PORT}
    worker:
      command: "./bin/worker"
```

Commands and working directories are native Process Compose configuration, not framework adapters. The backend overrides its process-local `PORT`; both processes inherit the default `PARCELA_URL` unless project configuration overrides an application-specific variable. Core does not discover or start npm workspaces automatically.

Each run records a service endpoint set under one generation and nonce. All declared HTTP services must pass startup readiness before any group route opens. A changed listener identity withdraws the group until every required endpoint passes readiness again. `status.live.endpoints` reports each service's listener ownership and fresh HTTP readiness. Whole-graph `down`, `restart` and qualified `recover` operate on the set. Individual service restart is not a Parcela command.

Registration reserves every service hostname atomically, including collisions with another worktree's base hostname. Port allocation reserves the whole set atomically. Service and named ports come from 20000–29999, chosen at random, below the OS ephemeral ranges (macOS 49152+, Linux 32768+) so other programs binding port 0 cannot take them before the service starts. A port in use on any loopback or wildcard address is skipped. Stopped hostnames stay reserved and unavailable. To change reservations, stop the environment, change its manifest and run `up` again with its existing identity. No source or data is removed. Distinct names and runtime paths do not certify database or storage isolation.

## Context and environment

`run/up/task` supply `PARCELA_PROJECT_ID`, `PARCELA_ENVIRONMENT_ID`, `PARCELA_ROLE`, `PARCELA_GENERATION`, `PARCELA_RUNTIME_DIR`, `PARCELA_URL`, `PARCELA_HOSTNAME`, `HOST=127.0.0.1`, and `PORT` when allocated/recorded. Each declared service also receives `PARCELA_SERVICE_<NAME>_URL` and `_HOSTNAME`, plus `_PORT` when allocated or recorded. Each named port supplies `PARCELA_PORT_<NAME>` when recorded. Names become uppercase and hyphens become underscores. `PORT`, `PARCELA_URL` and `PARCELA_HOSTNAME` remain default-service aliases. `--backend-port` selects only the default service's port; other service ports are allocated automatically. These reserved keys cannot be overridden by organizer configuration. Manifest environment values support `${...}` substitution from the supplied context; unresolved placeholders fail.

`inherit_environment` defaults to true. With false, PATH/HOME are retained and other inherited values are cleared. Inherited `PC_*` and `PARCELA_*` controls are removed before resolved context is applied. This is not sandboxing: project code can read files, load dotenv and make network requests.

The private runtime has `data`, `tmp`, `cache` and `log`. Projects create any other directories they need, such as uploads or PID-file parents. Core creates paths, not database schemas or baseline data. `context.json` records the launch generation. Tasks write `last-task.json`. Projects must consume paths explicitly; `up` never invokes preparation automatically.

## Commands

| Command | Behavior |
|---|---|
| `init [--name NAME] [--command CMD]` | Write a commented short-form `parcela.yaml` in `--path`, guessing the web command from `package.json`, Rails or Django files; refuses to overwrite; validates before writing |
| `up [--name NAME\|--primary] [--detach] [--backend-port N]` | Start every service, foreground by default; detached startup returns only after live readiness and prints the URL and next commands |
| `down` / `stop` | Stop through the live supervisor or authenticated recovery; preserve source/data; repeated stop is a no-op |
| `down --all` / `down --all-projects` | Stop every running environment of this project, or of every project; prints the targets first and exits 1 if any stop fails |
| `restart [--detach] [--backend-port N]` | Check the replacement before stopping the healthy run, then start a new run; does not replay arbitrary `run` argv |
| `status [--all]` | Live supervisor, loopback HTTP and router probes, with recorded state reported separately |
| `ls` | Every registered environment across projects: project, name, state, URL and path |
| `services` | Process Compose state of each service and worker |
| `logs [SERVICE] --tail N` | Plain-text log tail; default is the routed service; 1–10,000 lines |
| `url [--service NAME]` | Plain-text default or selected service URL; does not certify reachability |
| `rm` | Forget the selected stopped environment and release its hostnames and port leases; refuses a running one; keeps source and runtime data |
| `rm --prune` | Forget every stopped environment whose checkout directory no longer exists; running ones are skipped |
| `doctor` | Readiness checklist: Process Compose and Caddy on `PATH`, `lsof`, `ps`, the nearest `parcela.yaml`, the private state directory and the proxy; creates no state |
| `config check` | Strict validation, including a Process Compose process for every HTTP service; creates no state |
| `run [--name NAME\|--primary] [--backend-port N] -- ARGV` | Literal foreground command, startup readiness and owned listener monitoring |
| `recover [--detach]` | Reattach to surviving authenticated services without replaying them; an interrupted stop remains a stop |
| `task NAME -- ARGS` | Literal project argv plus literal appended args; optional timeout |
| `context` / `workspace inspect` | Planned context; no live-health guarantees |
| `workspace adopt --role primary\|preview --label LABEL` | Optional explicit registration; `up` does this automatically |
| `workspace list [--all]` | This project's registered environments, or every project's |
| `workspace discover` | Read-only sibling jj/Git checkout paths, revisions, refs and registration IDs; no manifest or registry required |
| `proxy run [--caddy PATH]` | Dedicated foreground Caddy; default 80/443; no sudo/trust/takeover |
| `proxy run --test-profile --http-port N --https-port N` | Explicit disposable high-port profile |
| `proxy run --forwarded-standard-ports --http-port 8080 --https-port 8443` | Explicit external 80/443 forwarding, canonical port-free URL |
| `proxy status` | Verify the owned Caddy admin marker; the mapped profile also verifies the port-80 front door |
| `--skill` / `skill` | Print the embedded agent operating skill as plain Markdown; no project, registry or backend access |

`parcela` with no command prints the current checkout's status when it is registered, and otherwise the next command to run. Rarely needed flags are accepted but hidden from help: `--human`, `doctor --caddy PATH --process-compose PATH`, `--process-compose PATH` on `up` and `restart`, and `--readiness-timeout SEC` on `up`, `restart` and `run`. They default to the binaries on `PATH` and 60 seconds (30 for `run`). `doctor` exits 1 until every check passes.

`--skill` is a long-flag alias for the `skill` subcommand. It runs before workspace lookup and cannot be combined with a lifecycle command. Its output is the bundled `.agents/skills/parcela/SKILL.md`, not generated from the current project. An agent can load this output directly; no global profile installation runs.

Every command prints aligned human text when stdout is a terminal and JSON otherwise; `--json` forces JSON. Color appears only on a terminal without `NO_COLOR`. `url` and `logs` always print plain text. Foreground child output passes through; detached owner output appends to private `log/owner.log`. Readiness timeouts accept 1–86,400 seconds. Normal child exit status propagates; termination uses conventional signal exit codes and task timeout returns 124. Operational failures exit 1; Clap argument failures exit 2. No implicit setup, sharing, arbitrary persisted-PID signaling, or replay of an uncertain command; bulk stop happens only through explicit `down --all` or `--all-projects`. A failed detached start prints the lines it added to `log/owner.log` to stderr.

## Ownership and failure

Registry schema 3 adds hostname and endpoint-set port reservations. Schema 4 records each session's named ports, which share the active port reservations. Writable opening migrates schemas 1 to 3 transactionally, preserving workspace IDs, session nonces, generations and active default-port leases. Read-only opening supports those older schemas without migration. Legacy single-service owner records remain readable without rewriting them. A stopped generation cannot transition back to ready; a new run needs a new generation.

Old binaries do not support schema 4. Upgrade the owned proxy and stop or explicitly reconcile existing owners before a new binary opens the installed registry for writing. A disposable registry migration is not an installed-host upgrade. Private state is user-owned, mode 700; files use mode 600 and final symlinks are rejected. Source aliases canonicalize to one identity. Conflicts roll back instead of taking ownership. Missing source paths are retained, not deleted.

Kernel lifetime locks protect each environment and the dedicated proxy. Session updates require nonce/generation identity. Full Caddy configuration updates are serialized. Backend listener PID ancestry is checked while the retained child is live; probe-and-release is not an atomic socket handoff. Changed listener identity withdraws the route and requires readiness again. The owner does not continuously poll HTTP readiness after establishment; `status` performs a fresh HTTP probe. A failed probe can report `unhealthy` while `recorded_session.status` still says `ready`. Native Process Compose remains authoritative for individual workers.

Stopped/unready registered hostnames answer unavailable; unknown hosts do not fall through to another owner. Route revocation precedes normal child teardown. Caddy routes require short-lived file leases renewed by the proxy guardian only while the recorded application owner and graph remain valid. Owner or guardian death expires new-request routing even if Caddy survives. Proxy outages leave application graphs running; a compatible front door can restore their routes without replacing application generations. Lease renewal does not require a Caddy configuration change. Existing streams are not forcibly closed merely because a lease expires.

Recovery requires a matching generation-scoped owner record, the original lock inode, a dead former controller, and authenticated native socket, token, and process-birth evidence. A qualified native graph can recover before HTTP readiness without replaying services. Its route remains unavailable until readiness succeeds. When the former controller and every recorded supervisor/descendant birth identity are gone, in any recorded phase, `down`, `recover` and `up` finish a stopped receipt without contacting the exited supervisor. They save the stopped phase, remove only inode-matching `ctl.sock`/`pc.sock`, and release the generation's leases. A later `up` starts a new generation. Nothing is signaled. A surviving recorded identity still requires authenticated native recovery. This covers owner and Process Compose death together (for example `kill -9` or reboot); it proves desired-state convergence, not the supervisor's exit code. Descendants that escaped the recorded tree are not detected.

Stop recovery callers that contend for the owner lock wait within the recorded shutdown budget for the selected generation to complete. If a live controller resumes that generation, the caller forwards stop intent through its recorded control socket. Forwarding requires matching controller birth and socket inode evidence, plus the held original lock. The durable stopping phase records admission. Generation and token checks run while waiting and after acquisition, so a stale caller cannot stop a successor. Only the elected controller requests native shutdown. File and permission errors still fail immediately. Non-stop recovery remains fail-fast on occupied ownership. Cleanup remains socket-inode-qualified.

Allocations without durable launch evidence retain their leases and refuse automatic cleanup or replay. Unknown or legacy owners, daemonized descendants, replaced handles, and arbitrary orphan graphs are not automatically adopted or signaled. Private paths are operational ownership boundaries, not containment against hostile same-user programs.

## Local control and discovery

Each new runtime owner holds `owner.lock` and binds private `ctl.sock`. Unix socket paths are capped near 100 bytes, so the control, Process Compose and Caddy admin sockets live outside the state directory, in `/tmp/parcela-<uid>/<hash>`, the same for every shell, cron job and login session. The hash is the first 8 hex digits of FNV-1a over the canonical state path. Both directory levels must be real, user-owned and mode 700; otherwise the command refuses. `context.json` records the run's socket paths. Requests and replies carry the environment ID, generation and nonce. Stop requests run inside that owner, which retains the actual child handle. The client waits for stopped intent and released ownership. Socket cleanup removes only the socket inode this owner created; existing/foreign sockets are never replaced. Malformed or stale requests cannot trigger an operation. Malformed health replies remain unknown.

Detached startup verifies that the answering owner is the retained child it just launched. Its output includes the launch generation and private log path. `up --detach` is explicit; it does not install a launch agent or boot service. `restart` restarts the whole graph, not one named service, and requires the selected backend to be available. Before stopping the current graph, it checks the replacement binary, native graph, and explicit backend endpoint. A busy endpoint is reusable only with matching routable ownership and authenticated native control. Other listeners, reserved endpoints, privileged ports, and router ports are refused without stopping the current graph. These checks do not make concurrent restart atomic. No arbitrary command replay or automatic restart policy is added to `run`.

`status` separates `recorded_session`, `state`, `live`, and `live_health_verified`. The verification flag means an owner answered a fresh health probe, not that its HTTP probe passed or all workers are healthy. Missing, legacy or interrupted owners report `unknown` with a diagnostic. Stopped/never-started sessions have no live verification. Inspection does not recover an owner or release an uncertain lease. Explicit lifecycle recovery uses the qualified evidence described above.

`workspace discover` finds the nearest VCS root and prefers jj over Git in a colocated root. jj uses `--ignore-working-copy --at-op=@`, so inventory neither snapshots edits nor merges operations. Its revisions/bookmarks describe the recorded operation, not unsnapshotted file contents. Git uses NUL-delimited read-only worktree inventory, including detached checkouts. Discovery never adopts a sibling or infers its primary/preview role. Plain directories remain valid. jj workspace-root templates were tested with 0.40.0 on macOS and 0.45.1 on Linux; unavailable capabilities produce an error, never a Git fallback.
