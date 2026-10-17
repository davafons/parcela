# Configure a service set

## Declare the HTTP endpoints

Put one `parcela.yaml` at the intended checkout root. Use an immutable project ID and a DNS-safe name. List only HTTP endpoints in `services`:

```yaml
version: 1
project: {id: my-app, name: my-app}
default_service: frontend
services:
  frontend: {readiness_path: /up}
  backend: {subdomain: api, readiness_path: /up}
```

The default service uses the environment base. `subdomain` supplies a flat prefix, despite its field name. Without it, a non-default service uses its own name as the prefix. Default services cannot set a prefix.

All HTTP endpoints are required for group startup readiness. Do not declare a worker, asset compiler or SDK watcher as an HTTP service unless it actually serves an endpoint.

## Supply the native process graph

Commands and working directories belong to Process Compose:

```yaml
process_compose:
  version: "0.5"
  processes:
    frontend:
      working_dir: frontend
      command: ./bin/dev --host ${HOST} --port ${PORT}
    backend:
      working_dir: backend
      command: ./bin/dev
      environment:
        - PORT=${PARCELA_SERVICE_BACKEND_PORT}
    worker:
      command: ./bin/worker
```

Replace these illustrative project commands with your actual entry points. A service set's HTTP service names must match native process names. Legacy single-service configurations can retain a different native process name.

Bind every HTTP listener to `127.0.0.1` and consume its assigned port. The default `HOST` is loopback. An HTTP answer from a foreign listener cannot establish owned readiness. Processes must stay supervised; daemonized descendants are not certified.

Native graph fields pass through to Process Compose. `config check` validates organizer fields, not the whole native graph. `up` also performs a native dry-run before launch. Use the selected Process Compose version's supported dependency, availability and shutdown settings.

## Bind application URLs and data

Use service context to supply an application's internal API origin:

```yaml
environment:
  APP_PUBLIC_URL: "${PARCELA_URL}"
  BACKEND_ORIGIN: "${PARCELA_SERVICE_BACKEND_URL}"
  APP_DATA: "${PARCELA_RUNTIME_DIR}/data"
```

Service names become uppercase in variable names; hyphens become underscores. Each service has `PARCELA_SERVICE_<NAME>_URL`, `_HOSTNAME` and an allocated `_PORT`. `PORT`, `PARCELA_URL` and `PARCELA_HOSTNAME` remain default-service aliases. Override process-local application variables explicitly when the backend needs different values. Process Compose interpolates `${PORT}` in command text before applying that process-local override. Use `${PARCELA_SERVICE_BACKEND_PORT}` directly in the command, or let the application read its local `PORT` environment variable.

Core creates private runtime directories, not application schemas or baseline data. Your code must consume the bindings. Define primary versus preview database, upload, secret and queue policy in the project. Runtime paths are operational ownership boundaries, not a sandbox.

`inherit_environment` defaults to true. With false, PATH and HOME remain and other inherited values are cleared. Parcela removes inherited `PORTLESS_*`, `PC_*` and `PARCELA_*` controls before applying its resolved context. Application code can still read files, load dotenv and make network requests.

## Choose readiness paths

Select a fast HTTP path that returns a 2xx response when that service can accept its intended traffic. Parcela probes the allocated loopback endpoint with its declared hostname.

After a listener identity changes, Parcela withdraws group routing and requires readiness again. It does not continuously poll HTTP health after establishment. `status` makes fresh HTTP probes and can report `unhealthy` while recorded intent still says `ready`.

Use native process checks for workers and application checks for behavior. Neither a Running process nor an HTTP liveness response proves a database migration, a queue's correctness or a complete UI workflow.

## Optionally define finite tasks

```yaml
tasks:
  test:
    argv: [./bin/test]
    timeout_seconds: 300
```

Tasks are optional. Keep using existing npm, Make or other project commands directly when they do not need Parcela context. Run a declared task with literal appended arguments:

```sh
parcela task test -- focused-example
```

Read its effects first. `up` never invokes a setup, migration, reset, seed or import task automatically. See [configuration and CLI](../configuration.md) for validation, timeouts, exit codes and the full context contract.
