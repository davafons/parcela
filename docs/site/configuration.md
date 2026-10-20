# Configuration

Parcela reads `parcela.yaml` from the root of your checkout. Commands look for it in the current directory, or `--path`, and then in each parent directory. Every field has one spelling and one shape. The format is version 1. Unknown keys are errors except top-level `x-` keys. [Configure your app](frameworks.md#services-and-setup) covers the setup workflow.

## version and project

```yaml
version: 1
project: shop
```

Both are required. `version` is the file format, and this Parcela reads `1`. A file without it, or with another number, is an error. `project` names the app in URLs, `https://shop.localhost:1443`, and groups every checkout of the repository. It is a DNS label: lowercase letters, digits and inner hyphens. `parcela init` writes both.

## services

`services` is required. At least one service must have `http`, and exactly one HTTP service must omit `subdomain`. Every long-running process is a service. Each one is a mapping with a `command`:

```yaml
services:
  web:
    command: bin/rails server -p $PORT -b 127.0.0.1
    http:
      healthcheck: /up
  jobs:
    command: bin/jobs
```

| Field | Meaning |
|---|---|
| `command` | Required. A shell command, run with `bash -c` (`sh -c` where there is no bash) from the checkout root. Its output goes to `parcela logs`. |
| `http` | Makes the service reachable at an HTTPS URL. See below. |
| `healthcheck` | For services without `http`: a shell command that exits 0 once the service is ready, like `pg_isready`. |
| `depends_on` | Services to start first. Parcela waits until each one is ready: its HTTP healthcheck answers, its `healthcheck` passes, or, without either, it has started. |
| `stop` | A shell command that stops what `command` started outside its own processes, like `docker compose stop`. See [stop](#stop). |
| `working_dir` | A directory relative to the checkout root to run `command` in. |
| `env` | Variables for this service only. Values can use `${NAME}`, like the top-level `env`. |

Service names are DNS labels. `setup` and `prestart` are reserved for hooks. Dependency cycles and unknown dependencies are errors. A service cannot declare both `http` and a shell `healthcheck`.

### stop

Parcela stops a service by signaling its processes. That is not enough when `command` only asks another program to run something, such as `docker compose up` asking Docker for containers: if the client is killed or crashes, the containers keep running. `stop` names the command that stops them:

```yaml
services:
  deps:
    command: exec docker compose up postgres
    healthcheck: docker compose exec postgres pg_isready
    stop: docker compose stop
```

- It runs whenever the environment stops: on `down`, `restart`, a failed `up` and Ctrl-C. It runs after every service that depends on this one has stopped, and before this service gets SIGTERM.
- It runs even if `command` already exited, and after Parcela itself crashed: the next `up`, `down` or `recover` runs it for a run that did not finish stopping.
- It gets the service's variables and `working_dir`, writes to `parcela logs <service>`, and has 120 seconds.
- If it fails, `down` says so and the environment stays interrupted. Run `parcela down` again, which runs it again. Write it so it can run twice.

Without `stop`, a service gets SIGTERM and, 10 seconds later, SIGKILL.

### HTTP services

A service with `http` gets a free port in `$PORT`, must listen on `127.0.0.1:$PORT`, and gets its own URL:

```yaml
services:
  web:
    command: npm run dev -- --port $PORT --host 127.0.0.1
    http:
      healthcheck: /
  api:
    command: bin/api --port $PORT
    http:
      subdomain: api
      healthcheck: /health
```

| Field | Meaning |
|---|---|
| `healthcheck` | Required. A path. `up` waits until `GET` on it returns 2xx, then opens the URLs. |
| `subdomain` | The prefix for this service's URL: `https://api-shop.localhost:1443`. Leave it out on exactly one HTTP service, which gets the environment's own URL, `https://shop.localhost:1443`. |
| `wildcard` | `true` also sends every subdomain of the service's host to it, for subdomain-per-tenant apps: `https://kobe.shop.localhost:1443`, and `https://kobe.signup-shop.localhost:1443` in the `signup` worktree. `.local` names from `--lan` cover the host only. |

Every environment of the project uses the same prefixes in front of its own name: the `signup` worktree's API is at `https://api-signup-shop.localhost:1443`.

## readiness_timeout_seconds

```yaml
readiness_timeout_seconds: 600
```

How long `up` and `restart` wait for every service to be ready, from 1 to 86400. The default is 60. Time spent in hooks does not count, but waiting for a dependency's `healthcheck` does, so raise it for services that are slow on their first start, such as a container image that is built first.

## hooks

Optional commands for preparation and removal. Each hook has a required `command`. `setup` and `prestart` also accept `depends_on`.

```yaml
hooks:
  setup:
    command: bin/setup
    depends_on: [db]
  prestart:
    command: bin/rails db:migrate
    depends_on: [db]
```

| Hook | Runs |
|---|---|
| `setup` | On the first `up` of an environment. Install packages, create and seed the database, copy secrets. `parcela up --setup` runs it again. |
| `prestart` | On every new start, after `setup`. An `up` that reuses an already-ready environment runs no hooks. Run migrations or anything that must happen before the app starts. |
| `teardown` | On `parcela rm`, after the environment stopped. Drop its database or delete data outside the checkout. It cannot use `depends_on`. For a checkout that is already gone, it runs from `PARCELA_RUNTIME_DIR` with the configuration of the last successful run. |

`setup` and `prestart` take a `command` and an optional `depends_on`. On `up`, Parcela starts the hooks' `depends_on` services, waits until they are ready, runs `setup` if it is due, then `prestart`, then starts every other service. A hook that exits nonzero stops `up` with its exit code and output, and no other service starts. `parcela logs setup` shows a hook's output again, until the next run.

A failed setup is retried on the next start. A successful setup is skipped unless `--setup` is given.

## ports

Extra ports for things that are not HTTP services, such as a database:

```yaml
ports: [db, cache]
```

- Each name arrives as `PARCELA_PORT_<NAME>`, uppercased with hyphens turned into underscores: `PARCELA_PORT_CACHE`.
- An environment keeps the same port across `down`, `up` and `restart` while it is free.
- No two running environments get the same port. Parcela does not route or check these ports.

Ports come from 20000 through 29999.

## env

Variables for every service, hook and task:

```yaml
env:
  DATABASE_URL: postgres://127.0.0.1:${PARCELA_PORT_DB}/app
  API_ORIGIN: ${PARCELA_SERVICE_API_URL}
```

Values can use `${NAME}` to insert any variable Parcela sets. A placeholder that does not resolve is an error. You cannot set `PORT`, `HOST` or names starting with `PARCELA_`. Commands inherit the launching shell's environment except names starting with `PARCELA_`. Service `env` overrides top-level `env`. Use Parcela-provided variables in placeholders. Arbitrary shell variables such as `${HOME}` are unavailable to interpolation.

## tasks

Commands you run with `parcela task NAME`. They get the environment's variables:

```yaml
tasks:
  test:
    command: bin/rails test
    timeout_seconds: 600
  psql:
    command: psql "$DATABASE_URL"
```

| Field | Meaning |
|---|---|
| `command` | Required. A shell command, run from the checkout root. |
| `timeout_seconds` | Parcela stops the task after this many seconds and exits 124. |

Arguments after `--` are appended. `parcela task test -- test/models` runs `bin/rails test test/models`. `timeout_seconds` must be a positive integer when set.

## Editor support

The file format has a JSON Schema at `https://parcela.run/schema/v1.json`. Editors with the YAML language server, such as VS Code with the Red Hat YAML extension, use it for completion and validation when the file starts with this line, which `parcela init` writes:

```yaml
# yaml-language-server: $schema=https://parcela.run/schema/v1.json
```

## Extension keys

Top-level keys that start with `x-` are ignored, as in Docker Compose. Use them for YAML anchors:

```yaml
x-ruby: &ruby
  working_dir: backend
services:
  web:
    <<: *ruby
    command: bin/rails server -p $PORT
    http:
      healthcheck: /up
```

Any other unknown key is an error.

## Environment variables Parcela sets

Every service, hook and task receives these. Service names are uppercased, with hyphens turned into underscores.

| Variable | Example |
|---|---|
| `PORT` | `29381`, the service's own port. Only HTTP services get one. In tasks it is the port of the service at the environment's URL. |
| `HOST` | `127.0.0.1` |
| `PARCELA_URL` | `https://shop.localhost:1443`, the environment's URL |
| `PARCELA_HOSTNAME` | `shop.localhost` |
| `PARCELA_SHARE_HOSTS` | `shop.local,mb.tail1234.ts.net`, the hostnames other devices use after `up --lan` or `--tailscale`. Empty otherwise. Used for dev-server host allow lists. |
| `PARCELA_SERVICE_<NAME>_URL` | `PARCELA_SERVICE_API_URL=https://api-shop.localhost:1443` |
| `PARCELA_SERVICE_<NAME>_HOSTNAME` | `PARCELA_SERVICE_API_HOSTNAME=api-shop.localhost` |
| `PARCELA_SERVICE_<NAME>_PORT` | `PARCELA_SERVICE_API_PORT=26798` |
| `PARCELA_PORT_<NAME>` | `PARCELA_PORT_DB=26953`, one per entry in `ports` |
| `PARCELA_ENVIRONMENT_NAME` | `primary`, or the worktree's name, like `signup` |
| `PARCELA_DATA_DIR` | The environment's private data directory |
| `PARCELA_RUNTIME_DIR` | The directory that holds `data`, `tmp`, `cache` and `log` |
| `PARCELA_PRIMARY_DIR` | The main checkout, where untracked files like `.env` usually live |
| `PARCELA_PROJECT_ID` | `shop` |
| `PARCELA_ENVIRONMENT_ID` | `7cceea8f-0fd3-4cdc-843f-0883f3c0fc5e` |
| `PARCELA_ROLE` | `primary` or `preview` |
| `PARCELA_GENERATION` | `7`, a counter that increases on every start |

## State locations

| Path | Contents |
|---|---|
| `~/.local/state/parcela/` | All of Parcela's state. Override it with `--state-dir`, `PARCELA_STATE_DIR` or `XDG_STATE_HOME`. |
| `…/registry.sqlite3` | Projects, environments, names and port leases. |
| `…/environments/<id>/` | One environment's `PARCELA_RUNTIME_DIR`: `data`, `tmp`, `cache`, `log/owner.log`, each process's `log/processes/<name>.log` and Parcela's own records. |
| `…/proxy/` | The proxy's Caddy config, `proxy.log` and the local certificate authority. |
| `$(getconf DARWIN_USER_TEMP_DIR)parcela/` on macOS, `/tmp/parcela-<uid>/` on Linux | Unix sockets for the running environments and the proxy. |

The state directory is private to your user. `rm` keeps an environment's directory. A later registration gets a new directory. Retained files are not automatically reused.
