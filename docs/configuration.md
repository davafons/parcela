# Configuration and CLI

Commands locate the nearest `parcela.yaml` at or above `--path`. The manifest directory is the checkout root. `--workspace UUID` selects an adopted source instead of `--path`; the two options conflict. Explicit UUID inspection and stop still work when the source directory is missing. Starting or restarting always requires the current valid manifest. Use the same project identity across checkouts; adoption supplies machine-local roles and labels.

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

Only one routed service is currently executable. Additional service declarations can describe planned URLs but `run/up` reject them. Put non-routed workers in the native graph. Services must bind their assigned port only on `127.0.0.1`; mere HTTP answers from foreign listeners cannot establish readiness.

## Context and environment

`run/up/task` supply `PARCELA_PROJECT_ID`, `PARCELA_ENVIRONMENT_ID`, `PARCELA_ROLE`, `PARCELA_GENERATION`, `PARCELA_RUNTIME_DIR`, `PARCELA_URL`, `PARCELA_HOSTNAME`, `HOST=127.0.0.1`, and `PORT` when allocated/recorded. These reserved keys cannot be overridden by organizer configuration. Manifest environment values support `${...}` substitution from the supplied context; unresolved placeholders fail.

`inherit_environment` defaults to true. With false, PATH/HOME are retained and other inherited values are cleared. Inherited `PORTLESS_*`, `PC_*` and `PARCELA_*` controls are removed before resolved context is applied. This is not sandboxing: project code can read files, load dotenv and make network requests.

The private runtime has `data`, `uploads`, `cache`, `tmp/pids` and `log`. Core creates paths, not database schemas or baseline data. `context.json` records the launch generation. Tasks write `last-task.json`. Projects must consume paths explicitly; `up` never invokes preparation automatically.

## Commands

| Command | Behavior |
|---|---|
| `config check` | Strict organizer validation, no state creation |
| `workspace adopt --role primary\|preview --label LABEL` | Explicit source identity and hostname reservation |
| `workspace list [--all] [--human]` | Registered inventory plus live owner probes, scoped to the current project or explicit machine-wide view |
| `workspace discover [--human]` | Read-only sibling jj/Git checkout paths, revisions, refs and adoption IDs; no manifest or registry required |
| `context` / `workspace inspect` | Planned context; no live-health guarantees |
| `proxy run [--caddy PATH]` | Dedicated foreground Caddy; default 80/443; no sudo/trust/takeover |
| `proxy run --test-profile --http-port N --https-port N` | Explicit disposable high-port profile |
| `proxy run --forwarded-standard-ports --http-port 8080 --https-port 8443` | Explicit external 80/443 forwarding, canonical port-free URL |
| `proxy status` | Verify owned Caddy admin marker; mapped profile also verifies port-80 front door |
| `run [--backend-port N] [--readiness-timeout SEC] -- ARGV` | Literal foreground command, startup readiness and owned listener monitoring |
| `up [--detach] [--process-compose PATH] [--backend-port N] [--readiness-timeout SEC]` | Native graph, foreground by default; detached startup returns only after live readiness |
| `down` / `stop` | Stop through the live owner or authenticated native recovery; preserve source/data; reconcile completed shutdown and make repeated completed stop a no-op |
| `recover [--detach]` | Reattach to a surviving authenticated native graph without replaying services; an interrupted stop remains a stop operation |
| `restart [--detach] [--process-compose PATH] [--backend-port N] [--readiness-timeout SEC]` | Preflight the replacement backend/native graph before stopping the healthy owner, then launch in a fresh generation; does not replay arbitrary `run` argv |
| `services [--human]` | Native Process Compose service state through the selected live owner |
| `logs [SERVICE] --tail N` | Plain-text log tail from Process Compose; default service is the owner's routed service; 1–10,000 lines, bounded response size |
| `url` | Plain-text recorded URL, or the planned standard HTTPS URL before the first launch; does not certify reachability |
| `task NAME -- ARGS` | Literal project argv plus literal appended args; optional timeout |
| `status [--all] [--human]` | Live owner/loopback HTTP/router probes, with durable intent reported separately; explicit machine-wide view works outside a project |
| `doctor [--caddy PATH] [--process-compose PATH]` | Capability report plus requested bounded version probes |

Inspection and detached startup print JSON by default. `--human` selects inventory/status/service tables. `url` and `logs` print plain text. Foreground child output passes through; detached owner output appends to private `log/owner.log`. Readiness timeouts accept 1–86,400 seconds. Normal child exit status propagates; termination uses conventional signal exit codes and task timeout returns 124. Operational failures exit 1; Clap argument failures exit 2. No implicit setup, sharing, bulk stop, arbitrary persisted-PID signaling, or replay of an uncertain command.

## Transfer policy and checkpoints

The optional strict `transfer` policy and paused capture/restore commands are documented in [transfer commands](transfer.md). Preflight does not create registry state or certify execution handoff. Source-only checkpoints do not capture declared application data or profiles.

## Ownership and failure

Registry schema 2 adds session/port leases; writable opening migrates schema 1 explicitly, preserving identities. Private state is user-owned, mode 700; files use mode 600 and final symlinks are rejected. Source aliases canonicalize to one identity. Conflicts roll back instead of taking ownership. Missing source paths are retained, not deleted.

Kernel lifetime locks protect each environment and the dedicated proxy. Session updates require nonce/generation identity. Full Caddy configuration updates are serialized. Backend listener PID ancestry is checked while the retained child is live; probe-and-release is not an atomic socket handoff. Changed listener identity withdraws the route and requires readiness again. The owner does not continuously poll HTTP readiness after establishment; `status` performs a fresh HTTP probe. A failed probe can report `unhealthy` while `recorded_session.status` still says `ready`. Native Process Compose remains authoritative for individual workers.

Stopped/unready registered hostnames answer unavailable; unknown hosts do not fall through to another owner. Route revocation precedes normal child teardown. Caddy routes require short-lived file leases renewed by the proxy guardian only while the recorded application owner and graph remain valid. Owner or guardian death expires new-request routing even if Caddy survives. Proxy outages leave application graphs running; a compatible front door can restore their routes without replacing application generations. Lease renewal does not require a Caddy configuration change. Existing streams are not forcibly closed merely because a lease expires.

Recovery requires a matching generation-scoped owner record, the original lock inode, a dead former controller, and authenticated native socket/token and process-birth evidence for surviving graphs. A durable stopping or stopped record whose tracked graph is quiescent can finish its stopped receipt and registry update without contacting an exited supervisor. This proves desired-state convergence, not the native supervisor's exit code after controller death. Stop recovery callers that contend for the owner lock wait within the recorded shutdown budget for the selected generation to complete. Generation and token checks run while waiting and after acquisition; a stale caller cannot stop a successor. Only the elected controller requests native shutdown. File/permission errors still fail immediately, and non-stop recovery remains fail-fast on occupied ownership. Cleanup remains socket-inode-qualified. Unknown/legacy owners, daemonized descendants, replaced handles and arbitrary orphan graphs are not automatically adopted or signaled. Private paths are operational ownership boundaries, not containment against hostile same-user programs.

## Local control and discovery

Each new runtime owner holds `owner.lock` and binds private `ctl.sock`. Requests and replies carry the environment ID, generation and nonce. Stop requests run inside that owner, which retains the actual child handle. The client waits for stopped intent and released ownership. Socket cleanup removes only the socket inode this owner created; existing/foreign sockets are never replaced. Malformed or stale requests cannot trigger an operation. Malformed health replies remain unknown.

Detached startup verifies that the answering owner is the retained child it just launched. Its output includes the launch generation and private log path. `up --detach` is explicit; it does not install a launch agent or boot service. `restart` restarts the whole graph, not one named service, and requires the selected backend to be available. No arbitrary command replay or automatic restart policy is added to `run`.

`status` separates `recorded_session`, `state`, `live`, and `live_health_verified`. The verification flag means an owner answered a fresh health probe, not that its HTTP probe passed or all workers are healthy. Missing, legacy or interrupted owners report `unknown` with a diagnostic. Stopped/never-started sessions have no live verification. Inspection does not recover an owner or release an uncertain lease. Explicit lifecycle recovery uses the qualified evidence described above.

`workspace discover` finds the nearest VCS root and prefers jj over Git in a colocated root. jj uses `--ignore-working-copy --at-op=@`, so inventory neither snapshots edits nor merges operations. Its revisions/bookmarks describe the recorded operation, not unsnapshotted file contents. Git uses NUL-delimited read-only worktree inventory, including detached checkouts. Discovery never adopts a sibling or infers its primary/preview role. Plain directories remain valid. jj workspace-root templates were tested with 0.40.0 on macOS and 0.45.1 on Linux; unavailable capabilities produce an error, never a Git fallback.
