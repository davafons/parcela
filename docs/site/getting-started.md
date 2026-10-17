# Getting started with Parcela

This guide takes you from nothing to a small app with three processes, running at named HTTPS URLs, with a second copy of it running next to the first.

## What you'll build

You'll build a project called `hello` with:

- a static page at `https://hello.localhost:1443`,
- a JSON API at `https://api-hello.localhost:1443`,
- a background worker that prints a line every five seconds.

Then you'll create a Git worktree called `signup` and run the same app from it at `https://signup-hello.localhost:1443`, while the first copy keeps running.

The finished files are [parcela.yaml](https://github.com/davafons/parcela/blob/main/examples/docs-demo/parcela.yaml), [index.html](https://github.com/davafons/parcela/blob/main/examples/docs-demo/index.html), [api.py](https://github.com/davafons/parcela/blob/main/examples/docs-demo/api.py) and [worker.py](https://github.com/davafons/parcela/blob/main/examples/docs-demo/worker.py).

## Assumptions

You need macOS or Linux, Python 3 and Git. The example app is plain Python, so there is nothing else to install. You don't need to know Caddy or Process Compose. Parcela drives them for you.

## Install Parcela

Install the `parcela` binary into `~/.local/bin`:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/davafons/parcela/releases/latest/download/parcela-installer.sh | sh
```

Parcela uses [Caddy](https://caddyserver.com/docs/install) 2.x for HTTPS and [Process Compose](https://f1bonacc1.github.io/process-compose/installation/) 1.x to run your processes. Install both:

```sh
# macOS
brew install caddy f1bonacc1/tap/process-compose

# Linux: install Caddy from your distribution or caddyserver.com, then
sh -c "$(curl -fsSL https://raw.githubusercontent.com/F1bonacc1/process-compose/main/scripts/get-pc.sh)" -- -b ~/.local/bin
```

To build from source instead, use Rust 1.89 or newer: `cargo install --git https://github.com/davafons/parcela --locked`.

## Create the app

Make a directory with one HTML page:

```sh
mkdir hello && cd hello
echo '<h1>Hello from Parcela</h1>' > index.html
```

Now ask Parcela whether everything is in place:

```sh
parcela doctor
```

```text
✓ process-compose  v1.122.0 at /opt/homebrew/bin/process-compose
✓ caddy            v2.11.7 at /opt/homebrew/bin/caddy
✓ lsof             /usr/sbin/lsof
✓ ps               /bin/ps
✗ config           no parcela.yaml in /Users/you/code/hello or above
✓ state            /Users/you/.local/state/parcela (private)
✓ proxy            not running; parcela up starts it on HTTPS on :1443
· HTTPS trust is not checked, and only parcela trust changes it. If the browser warns, run parcela trust once.

Next step: parcela init
```

Every line marked ✗ says what to do next. Here the only thing missing is the project file.

## Write parcela.yaml

```sh
parcela init
```

```text
✓ Created /Users/you/code/hello/parcela.yaml
  web  python3 -m http.server $PORT --bind 127.0.0.1
  url  https://hello.localhost:1443

Next: parcela up
```

`init` looks for `package.json`, a Rails app or Django to guess your dev server. This directory has none of those, so it serves the directory with Python. Open `parcela.yaml`:

```yaml
# Parcela runs each service at its own HTTPS URL. Reference: https://github.com/davafons/parcela/blob/main/docs/configuration.md
# Services must listen on 127.0.0.1:$PORT.
# No dev server detected; this serves the directory, replace it with yours
web: python3 -m http.server $PORT --bind 127.0.0.1
```

That one line is a service called `web` and the command that starts it. Parcela picks a free port and passes it in `$PORT`. The project is named after the directory, `hello`.

## Start it

```sh
parcela up
```

```text
parcela: Started the Parcela proxy on :1443 (run `parcela trust` once so browsers trust https://hello.localhost:1443)
[web	] 127.0.0.1 - - [17/Oct/2026 20:40:13] "GET / HTTP/1.1" 200 -
parcela: ready https://hello.localhost:1443 (generation 2, routing-only; data isolation requires project bindings)
```

The first time, `up` also starts Parcela's HTTPS proxy in the background. The proxy keeps running for every project until you run `parcela proxy stop`.

Open `https://hello.localhost:1443` in your browser. You see "Hello from Parcela". The browser warns about the certificate first. The next section fixes that.

Press Ctrl-C to stop the app.

## Trust the certificate

Parcela's proxy signs certificates with its own local certificate authority. Tell your system to trust it once:

```sh
parcela trust
```

```text
parcela: adding Parcela's local certificate authority to the system trust store, so browsers accept its https://*.localhost URLs.
  certificate  /Users/you/.local/state/parcela/proxy/data/pki/authorities/local/root.crt
  command      caddy trust (Caddy may ask for your password)
  undo         caddy untrust --cert /Users/you/.local/state/parcela/proxy/data/pki/authorities/local/root.crt
parcela: trusted. Restart the browser if it still warns.
```

To use the certificate with `curl` without trusting it system-wide, pass its path:

```sh
curl --cacert "$(parcela trust --print)" https://hello.localhost:1443
```

```text
<h1>Hello from Parcela</h1>
```

## Run it in the background

Most of the time you want your terminal back. Start the app detached:

```sh
parcela up --detach
```

```text
✓ hello is running at https://hello.localhost:1443

  parcela logs    show service output
  parcela status  check health
  parcela down    stop it
```

`--detach` returns once every service answers. Check on it any time:

```sh
parcela status
```

```text
hello primary  ready
  https://hello.localhost:1443
  path /Users/you/code/hello
```

`primary` is the name of this environment. The main checkout of a project is `primary` unless you name it.

## Add an API

Create `api.py`. It answers every request with JSON that includes its own URL:

```python
import json
import os
from http.server import BaseHTTPRequestHandler, HTTPServer


class Hello(BaseHTTPRequestHandler):
    def do_GET(self):
        body = {"message": "hello from the API", "url": os.environ["PARCELA_SERVICE_API_URL"]}
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(body).encode() + b"\n")


HTTPServer(("127.0.0.1", int(os.environ["PORT"])), Hello).serve_forever()
```

Add it to `parcela.yaml` as a second service. You can delete the comment lines:

```yaml
web: python3 -m http.server $PORT --bind 127.0.0.1
api: python3 api.py
```

Every service gets its own `PORT` and its own hostname. The `api` service is served at `api-` plus the environment's hostname.

A new service needs a new hostname, so stop the environment and start it again:

```sh
parcela down
parcela up --detach
```

```text
✓ Stopped hello; source and data preserved
✓ hello is running at https://hello.localhost:1443
  api       https://api-hello.localhost:1443

  parcela logs    show service output
  parcela status  check health
  parcela down    stop it
```

```sh
curl https://api-hello.localhost:1443
```

```text
{"message": "hello from the API", "url": "https://api-hello.localhost:1443"}
```

The API read its URL from `PARCELA_SERVICE_API_URL`. Parcela sets a variable like this for every service, so your services can find each other. The [configuration reference](../configuration.md#environment-variables-parcela-sets) lists them all.

## Add a worker

A worker is a process with no port and no URL, like a job queue. Create `worker.py`:

```python
import time

tick = 0
while True:
    tick += 1
    print(f"tick {tick}", flush=True)
    time.sleep(5)
```

Add it under `workers`:

```yaml
web: python3 -m http.server $PORT --bind 127.0.0.1
api: python3 api.py
workers:
  ticker: python3 worker.py
```

This time no hostname changes, so a restart is enough. `restart` stops everything and starts it again from the current `parcela.yaml`:

```sh
parcela restart --detach
```

Wait a few seconds and read the worker's output:

```sh
parcela logs ticker --tail 3
```

```text
tick 1
tick 2
tick 3
```

## Give each checkout its own database port

Real apps also need ports that are not web servers: a database, Redis or a dev-server websocket. Two checkouts must not share them. List them under `ports` and Parcela leases one for each name:

```yaml
web: python3 -m http.server $PORT --bind 127.0.0.1
api: python3 api.py
workers:
  ticker: python3 worker.py
ports: [db]
environment:
  DATABASE_URL: postgres://127.0.0.1:${PARCELA_PORT_DB}/hello
tasks:
  db-url:
    argv: [sh, -c, 'echo "$DATABASE_URL"']
```

The `db` port arrives as `PARCELA_PORT_DB`. The `environment` block builds a `DATABASE_URL` from it for every process. The `db-url` task is a small command you can run with the environment's variables. In a real project you would start Postgres as a worker on `${PARCELA_PORT_DB}`.

```sh
parcela restart --detach
parcela status
```

```text
hello primary  ready
  https://hello.localhost:1443
  SERVICE  HTTP   URL
  api      ready  https://api-hello.localhost:1443
  web      ready  https://hello.localhost:1443
  db        21565  PARCELA_PORT_DB
  path /Users/you/code/hello
```

```sh
parcela task db-url
```

```text
postgres://127.0.0.1:21565/hello
```

The port stays the same across restarts as long as it is free.

## Run a second checkout in parallel

Commit the app and create a Git worktree called `signup` next to it:

```sh
git init -q && git add . && git commit -qm "hello app"
git worktree add ../signup
```

```text
Preparing worktree (new branch 'signup')
HEAD is now at 658dadc hello app
```

With jj, run `jj workspace add ../signup` instead. Start the app from the new checkout:

```sh
cd ../signup
parcela up --detach
```

```text
✓ signup (hello) is running at https://signup-hello.localhost:1443
  api       https://api-signup-hello.localhost:1443

  parcela logs    show service output
  parcela status  check health
  parcela down    stop it
```

The worktree belongs to the same project, `hello`, and gets its own name, `signup`. Its services, ports and logs are separate from the main checkout's:

```sh
curl https://api-signup-hello.localhost:1443
parcela task db-url
```

```text
{"message": "hello from the API", "url": "https://api-signup-hello.localhost:1443"}
postgres://127.0.0.1:28477/hello
```

See both environments:

```sh
parcela ls
```

```text
PROJECT  NAME     STATE  URL                                   PATH
hello    signup   ready  https://signup-hello.localhost:1443  /Users/you/code/signup
hello    primary  ready  https://hello.localhost:1443         /Users/you/code/hello
```

This is how several agents work on one project at the same time. Each one gets a worktree and runs the whole app in it.

## Stop and clean up

Stop every environment of the project:

```sh
parcela down --all
```

```text
Stopping 2 environments:
  NAME            URL
  signup (hello)  https://signup-hello.localhost:1443
  hello           https://hello.localhost:1443
✓ Stopped signup (hello); source and data preserved
✓ Stopped hello; source and data preserved
```

Remove the worktree, then tell Parcela to forget checkouts that no longer exist:

```sh
cd ../hello
git worktree remove ../signup
parcela rm --prune
```

```text
✓ Forgot signup (hello); released api-signup-hello.localhost, signup-hello.localhost
  data kept in /Users/you/.local/state/parcela/environments/c8af904e-1dff-495a-b4ce-0b163c9b2077
```

`down` and `rm` never delete your source or data. The proxy keeps running in the background. Stop it with `parcela proxy stop` if you want.

## Next steps

- [Parallel worktrees and agents](worktrees.md) covers naming, data separation and setting up a coding agent.
- The [CLI guide](cli.md) explains every command.
- The [configuration reference](../configuration.md) lists every `parcela.yaml` field.
