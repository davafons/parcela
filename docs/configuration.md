# Configuration and CLI

Commands locate the nearest `parcela.yaml` at or above `--path`. The manifest directory is the checkout root. Use the same project identity across checkouts; adoption supplies machine-local roles and labels.

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
| `workspace list [--all]` | Current project or explicit registry-wide inventory |
| `context` / `workspace inspect` | Planned context; no live-health guarantees |
| `proxy run [--caddy PATH]` | Dedicated foreground Caddy; default 80/443; no sudo/trust/takeover |
| `proxy run --test-profile --http-port N --https-port N` | Explicit disposable high-port profile |
| `proxy run --forwarded-standard-ports --http-port 8080 --https-port 8443` | Explicit external 80/443 forwarding, canonical port-free URL |
| `proxy status` | Verify owned Caddy admin marker; mapped profile also verifies port-80 front door |
| `run [--backend-port N] [--readiness-timeout SEC] -- ARGV` | Literal foreground command, startup readiness and owned listener monitoring |
| `up [--process-compose PATH] [--backend-port N] [--readiness-timeout SEC]` | Native foreground graph with explicit private Unix-socket mode, dotenv/watch/TUI disabled |
| `task NAME -- ARGS` | Literal project argv plus literal appended args; optional timeout |
| `status` | Recorded session intent, explicitly not live health |
| `doctor [--caddy PATH] [--process-compose PATH]` | Capability report plus requested bounded version probes |

Inspection prints JSON; child output from lifecycle commands passes through. Normal child exit status propagates; termination uses conventional signal exit codes and task timeout returns 124. Operational failures exit 1; Clap argument failures exit 2. No `stop`, detached operation, sharing or arbitrary persisted-PID signaling.

## Ownership and failure

Registry schema 2 adds session/port leases; writable opening migrates schema 1 explicitly, preserving identities. Private state is user-owned, mode 700; files use mode 600 and final symlinks are rejected. Source aliases canonicalize to one identity. Conflicts roll back instead of taking ownership. Missing source paths are retained, not deleted.

Kernel lifetime locks protect each environment and the dedicated proxy. Session updates require nonce/generation identity. Full Caddy configuration updates are serialized. Backend listener PID ancestry is checked while the retained child is live; probe-and-release is not an atomic socket handoff. Changed listener identity withdraws the route and requires readiness again. HTTP readiness is not continuously polled after establishment.

Stopped/unready registered hostnames answer unavailable; unknown hosts do not fall through to another owner. Route revocation precedes normal child teardown. Failed revocation retains the lease and reports failure. Crash recovery, daemonized descendants and unknown stale sockets are not automatically reconciled. Private paths are operational ownership boundaries, not containment against hostile same-user programs.
