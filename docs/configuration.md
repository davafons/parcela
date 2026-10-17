# Configuration

Parcela reads `parcela.yaml` from the checkout root. Commands find it by looking in the current directory, or `--path`, and then in each parent directory. This page lists every field. The [CLI guide](site/cli.md) covers the commands.

## Short form

Most projects need one line. `parcela init` writes it:

```yaml
web: npm run dev -- --port $PORT --host 127.0.0.1
```

Each top-level `name: command` is an HTTP service. The command must listen on `127.0.0.1:$PORT`.

## Services

Several services look like this:

```yaml
web: bin/rails server -p $PORT -b 127.0.0.1
api: node api/server.js --port $PORT
```

Or, the same thing under `services`:

```yaml
services:
  web: bin/rails server -p $PORT -b 127.0.0.1
  api: node api/server.js --port $PORT
```

Use one style per file, not both.

- Service names are DNS labels: lowercase letters, digits and inner hyphens.
- Every service gets its own free port in `$PORT`.
- The default service gets the environment's URL, for example `https://hello.localhost:1443`. It is `web` when there is one, otherwise the only service. With several services and no `web`, set `default_service: NAME`.
- Every other service gets its name in front: `https://api-hello.localhost:1443`.

A service can be a mapping when it needs more:

```yaml
services:
  web: bin/dev
  api:
    command: node api.js --port $PORT
    subdomain: backend
    readiness_path: /up
```

| Field | Default | Meaning |
|---|---|---|
| `command` | required | The shell command that starts the service. |
| `subdomain` | the service name | The prefix for this service's hostname. Not allowed on the default service. |
| `readiness_path` | `/` | Parcela waits until `GET` on this path returns 2xx before it opens the URLs. |

`up` waits for every service to be ready, then routes them all together.

## Workers

Workers are processes without a port or a URL:

```yaml
web: bin/dev
workers:
  jobs: bin/jobs
  css: npm run watch:css
```

A worker cannot have the same name as a service. `up`, `restart` and `down` start and stop workers with the services.

## Ports

List extra ports your processes need, such as a database, Redis or a dev-server websocket. Parcela leases a free port for each name on every run:

```yaml
web: npm run dev -- --port $PORT --host 127.0.0.1 --strictPort
workers:
  redis: redis-server --port ${PARCELA_PORT_REDIS}
ports: [redis, vite-hmr]
```

- The port arrives as `PARCELA_PORT_<NAME>`, uppercased with hyphens turned into underscores. `vite-hmr` becomes `PARCELA_PORT_VITE_HMR`.
- Every process and task receives it, and `environment` values can use it.
- A run reuses the previous port for each name while it is free. A database URL survives `down`, `up` and `restart`.
- No two running environments get the same port. Parcela does not route or check these ports.

Service and named ports come from 20000–29999.

## Environment

`environment` sets variables for every process and task. Values can use `${NAME}` to insert any variable Parcela sets:

```yaml
environment:
  DATABASE_URL: postgres://127.0.0.1:${PARCELA_PORT_DB}/app
  API_ORIGIN: ${PARCELA_SERVICE_API_URL}
  UPLOADS_DIR: ${PARCELA_RUNTIME_DIR}/data/uploads
```

A placeholder that does not resolve is an error. You cannot override the variables Parcela sets.

Processes inherit your shell's environment. Set `inherit_environment: false` to start from a clean environment. `PATH` and `HOME` are kept.

## Tasks

Tasks are commands you run with `parcela task NAME`. They get the environment's variables:

```yaml
tasks:
  test:
    argv: [bin/test]
    timeout_seconds: 300
  psql:
    argv: [sh, -c, 'psql "$DATABASE_URL" "$@"', psql]
```

| Field | Meaning |
|---|---|
| `argv` | The command and its arguments as a list. No shell runs unless you call one. |
| `timeout_seconds` | Optional. Parcela stops the task after this many seconds and exits 124. |

`parcela task psql -- -c 'select 1'` appends `-c 'select 1'` to `argv`. `up` never runs a task by itself.

## Other top-level fields

| Field | Default | Meaning |
|---|---|---|
| `project` | the main checkout's directory name | The project name. Write `project: shop`, or `project: {id: shop, name: shop}` to set the ID and the name separately. All jj workspaces and Git worktrees of a repository share it. |
| `default_service` | `web`, or the only service | The service that gets the environment's URL. |
| `shutdown_timeout_seconds` | `300` | How long `down` waits for processes to exit. 1 to 86400. |
| `version` | `1` | The file format version. |

A top-level key that looks like a misspelled field, such as `defualt_service`, is an error with a suggestion, not a service.

## Full form

The short form generates a [Process Compose](https://f1bonacc1.github.io/process-compose/) configuration for you. When you need Process Compose features such as `working_dir`, `depends_on` or per-process `environment`, write `process_compose` yourself and list the HTTP services without commands:

```yaml
project: shop
default_service: frontend
services:
  frontend: {readiness_path: /}
  backend: {subdomain: api, readiness_path: /up}
process_compose:
  version: "0.5"
  processes:
    frontend:
      working_dir: frontend
      command: npm run dev -- --port ${PORT} --host ${HOST}
    backend:
      working_dir: backend
      command: bin/dev --port ${PARCELA_SERVICE_BACKEND_PORT}
      environment:
        - PORT=${PARCELA_SERVICE_BACKEND_PORT}
    worker:
      working_dir: backend
      command: bin/jobs
```

- Every name under `services` must also be a process. Processes not in `services` are workers.
- Parcela passes `process_compose` to Process Compose unchanged, apart from adding its own variables.
- Process Compose expands `${PORT}` in commands from the shared environment, where it is the default service's port. In another service, use `${PARCELA_SERVICE_<NAME>_PORT}` and set `PORT` in that process's `environment`. The short form does this for you.

## Environment variables Parcela sets

Every process and task receives these. Service names are uppercased, with hyphens turned into underscores.

| Variable | Example |
|---|---|
| `PORT` | `29381`. The process's own port in the short form, the default service's port in shared expansion. |
| `HOST` | `127.0.0.1` |
| `PARCELA_URL` | `https://hello.localhost:1443`, the default service's URL |
| `PARCELA_HOSTNAME` | `hello.localhost` |
| `PARCELA_SERVICE_<NAME>_URL` | `PARCELA_SERVICE_API_URL=https://api-hello.localhost:1443` |
| `PARCELA_SERVICE_<NAME>_HOSTNAME` | `PARCELA_SERVICE_API_HOSTNAME=api-hello.localhost` |
| `PARCELA_SERVICE_<NAME>_PORT` | `PARCELA_SERVICE_API_PORT=26798` |
| `PARCELA_PORT_<NAME>` | `PARCELA_PORT_DB=26953`, one per entry in `ports` |
| `PARCELA_RUNTIME_DIR` | The environment's private directory, with `data`, `tmp`, `cache` and `log` inside |
| `PARCELA_PROJECT_ID` | `hello` |
| `PARCELA_ENVIRONMENT_ID` | `7cceea8f-0fd3-4cdc-843f-0883f3c0fc5e` |
| `PARCELA_ROLE` | `primary` or `preview` |
| `PARCELA_GENERATION` | `7`, a counter that increases on every start |

## State locations

| Path | Contents |
|---|---|
| `~/.local/state/parcela/` | All of Parcela's state. Override it with `--state-dir`, `PARCELA_STATE_DIR` or `XDG_STATE_HOME`. |
| `…/registry.sqlite3` | Projects, environments, names and port leases. |
| `…/environments/<id>/` | One environment's `PARCELA_RUNTIME_DIR`: `data`, `tmp`, `cache`, `log/owner.log` and Parcela's own records. |
| `…/proxy/` | The proxy's Caddy config, `proxy.log` and the local certificate authority. |
| `/tmp/parcela-<uid>/` | Unix sockets for the running environments and the proxy. |

The state directory is private to your user. `rm` keeps an environment's directory. Delete it yourself when you no longer need its data.
