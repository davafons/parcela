# Configuration

Parcela reads `parcela.yaml` from the root of your checkout. Commands look for it in the current directory, or `--path`, and then in each parent directory. Every field has one spelling and one shape. The [CLI guide](site/cli.md) covers the commands.

## A complete example

A Rails app with its own PostgreSQL server in every environment:

```yaml
version: 1
project: shop

env:
  DATABASE_URL: postgres://postgres@127.0.0.1:${PARCELA_PORT_DB}/shop_development

ports: [db]

services:
  db:
    command: >-
      test -d "$PARCELA_DATA_DIR/postgres" || initdb -D "$PARCELA_DATA_DIR/postgres" -U postgres --auth=trust;
      exec postgres -D "$PARCELA_DATA_DIR/postgres" -p "$PARCELA_PORT_DB" -c listen_addresses=127.0.0.1 -k ''
    healthcheck: pg_isready -h 127.0.0.1 -p "$PARCELA_PORT_DB"
  web:
    command: bin/rails server -p $PORT -b 127.0.0.1
    http:
      healthcheck: /up
    depends_on: [db]
  jobs:
    command: bin/jobs
    depends_on: [db]

hooks:
  setup:
    command: bundle install && bin/rails db:prepare
    depends_on: [db]
  prestart:
    command: bin/rails db:migrate
    depends_on: [db]

tasks:
  console:
    command: bin/rails console
```

The first `parcela up` in a new worktree creates its database, prepares it and starts the app. Later runs only migrate. The file is in [examples/rails-postgres](https://github.com/davafons/parcela/blob/main/examples/rails-postgres/parcela.yaml).

## version and project

```yaml
version: 1
project: shop
```

Both are required. `version` is the file format, and this Parcela reads `1`. A file without it, or with another number, is an error. `project` names the app in URLs, `https://shop.localhost:1443`, and groups every checkout of the repository. It is a DNS label: lowercase letters, digits and inner hyphens. `parcela init` writes both.

## services

Every long-running process is a service. Each one is a mapping with a `command`:

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
| `command` | Required. A shell command, run from the checkout root. |
| `http` | Makes the service reachable at an HTTPS URL. See below. |
| `healthcheck` | For services without `http`: a shell command that exits 0 once the service is ready, like `pg_isready`. |
| `depends_on` | Services to start first. Parcela waits until each one is ready: its HTTP healthcheck answers, its `healthcheck` passes, or, without either, it has started. |
| `working_dir` | A directory relative to the checkout root to run `command` in. |
| `env` | Variables for this service only. Values can use `${NAME}`, like the top-level `env`. |

Service names are DNS labels. `setup` and `prestart` are taken by hooks.

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

Every environment of the project uses the same prefixes in front of its own name: the `signup` worktree's API is at `https://api-signup-shop.localhost:1443`.

## hooks

Hooks are commands that prepare an environment. They are how `parcela up` alone takes a fresh worktree to a running app.

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
| `prestart` | On every `up` and `restart`, after `setup`. Run migrations or anything that must happen before the app starts. |

Each hook has a `command` and an optional `depends_on`. On `up`, Parcela starts the hooks' `depends_on` services, waits until they are ready, runs `setup` if it is due, then `prestart`, then starts every other service. A hook that exits nonzero stops `up` with its exit code and output, and no other service starts. While the environment runs, `parcela logs setup` shows a hook's output again.

A setup that fails runs again on the next `up`. Write it so it can run twice, like `bin/rails db:prepare` or `CREATE TABLE IF NOT EXISTS`.

Untracked files, such as `.env`, are not in a new worktree. Copy them from the main checkout in `setup`:

```yaml
hooks:
  setup:
    command: cp -n "$PARCELA_PRIMARY_DIR/.env" .env; bundle install
```

## Databases

Every environment needs its own data, or two worktrees overwrite each other's. Three ways to do that:

**SQLite.** Put the file in the environment's data directory and create it in `setup`:

```yaml
env:
  DATABASE_PATH: ${PARCELA_DATA_DIR}/app.sqlite3
hooks:
  setup:
    command: bin/rails db:prepare
```

**A database server per environment.** Run it as a service on a named port, as in [the complete example](#a-complete-example). Its files live in `$PARCELA_DATA_DIR`, and `down` stops it with the app.

**One shared server, one database per environment.** Name the database after the environment:

```yaml
env:
  DATABASE_URL: postgres://127.0.0.1/shop_${PARCELA_ENVIRONMENT_NAME}
hooks:
  setup:
    command: bin/rails db:prepare
```

The environment name is `primary` for the main checkout and the worktree's name otherwise, so it can contain hyphens.

## ports

Extra ports for things that are not HTTP services, such as a database or a dev-server websocket:

```yaml
ports: [db, vite-hmr]
```

- Each name arrives as `PARCELA_PORT_<NAME>`, uppercased with hyphens turned into underscores: `PARCELA_PORT_VITE_HMR`.
- An environment keeps the same port across `down`, `up` and `restart` while it is free.
- No two running environments get the same port. Parcela does not route or check these ports.

Ports come from 20000–29999.

## env

Variables for every service, hook and task:

```yaml
env:
  DATABASE_URL: postgres://127.0.0.1:${PARCELA_PORT_DB}/app
  API_ORIGIN: ${PARCELA_SERVICE_API_URL}
```

Values can use `${NAME}` to insert any variable Parcela sets. A placeholder that does not resolve is an error. You cannot set `PORT`, `HOST` or names starting with `PARCELA_`. Commands also inherit the environment of the shell that ran `parcela up`.

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

Arguments after `--` are appended: `parcela task test -- test/models` runs `bin/rails test test/models`.

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
| `…/environments/<id>/` | One environment's `PARCELA_RUNTIME_DIR`: `data`, `tmp`, `cache`, `log/owner.log` and Parcela's own records. |
| `…/proxy/` | The proxy's Caddy config, `proxy.log` and the local certificate authority. |
| `/tmp/parcela-<uid>/` | Unix sockets for the running environments and the proxy. |

The state directory is private to your user. `rm` keeps an environment's directory. Delete it yourself when you no longer need its data.
