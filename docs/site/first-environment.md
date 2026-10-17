# Run your first environment

We will run a static frontend, a separate API origin and a worker in one environment. The demo has no database, setup task or external worker effects.

Complete [build and host setup](install.md) first. Keep the owned proxy running in a separate shell. These commands assume a working standard HTTPS front door and its explicitly trusted CA.

## Prepare the demo

Set `PARCELA_SOURCE` to your Parcela source checkout. Choose a new demo directory whose parent already exists:

```sh
PARCELA_SOURCE=/absolute/path/to/parcela
DEMO=/absolute/path/to/new-parcela-demo

test ! -e "$DEMO" && \
  cp -R "$PARCELA_SOURCE/examples/docs-demo" "$DEMO" && \
  cd "$DEMO"
```

Do not copy over an existing directory. The same three source files are available as downloads: [manifest](../../examples/docs-demo/parcela.yaml), [frontend HTML](../../examples/docs-demo/frontend/index.html) and [backend JSON](../../examples/docs-demo/backend/status.json). Keep their directories as shown:

```text
parcela.yaml
frontend/index.html
backend/status.json
```

The manifest declares two HTTP services and three native processes. The frontend command consumes the default `PORT`. The backend command uses `PARCELA_SERVICE_BACKEND_PORT` directly and also sets its process-local `PORT`. Process Compose interpolates command text before applying process-local environment overrides. The worker has no HTTP route.

## Validate the configuration

```sh
parcela config check
```

The response has `valid: true`. This does not start Python, register an environment or verify native graph compatibility.

The project file defines the frontend, API and worker commands. Startup registers only this selected directory. A plain directory or main VCS checkout uses the base URL; a linked checkout uses a preview name. For a separate named copy, use `parcela up --name review --detach` as described in [preview startup](worktrees.md#adopt-an-existing-preview).

If the project base is already owned elsewhere, startup refuses rather than taking it over. Choose a preview name instead.

## Start the graph

```sh
parcela up --detach
```

No separate adoption command is required. Detached startup returns after both HTTP endpoints are ready. Its JSON includes the current generation, endpoint set, live observations and owner log path.

Inspect the processes and endpoints:

```sh
parcela context
parcela status
parcela services
parcela url
parcela url --service backend
```

The process list should report `frontend`, `backend` and `worker` as `Running`. Verify `status.state` is `ready` and both `live.endpoints` have owned listeners and successful HTTP probes.

## Check the actual HTTPS paths

```sh
FRONTEND_URL=$(parcela url)
API_URL=$(parcela url --service backend)

curl --noproxy '*' --fail "$FRONTEND_URL/"
curl --noproxy '*' --fail "$API_URL/status.json"
```

The frontend response contains `Parcela demo frontend`. The API returns `{"service":"backend","ready":true}`. These requests retain normal certificate verification. A test profile requires its explicit CA and returned port-qualified URLs instead.

Inspect the worker log:

```sh
parcela logs worker --tail 10
```

The log contains `demo worker ready`. A worker's process state is not a substitute for an application's functional check.

## Stop without deleting the demo

```sh
parcela down
parcela status
```

Status reports `stopped`. Source and private runtime data remain. The registered HTTPS names are unavailable while stopped; no other environment is stopped.

Next, [manage another worktree](worktrees.md) or [configure your own service set](services.md).
