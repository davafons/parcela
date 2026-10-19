# Getting started with Parcela

## Let your agent set it up

Most people start here. Paste this into Claude Code, Codex or any coding agent, from the root of your project:

```text
Set up Parcela for this project, so that `parcela up` alone runs the whole
app in any checkout or worktree, each at its own HTTPS URL.

1. If `parcela --version` fails, install it:
   curl -fsSL https://parcela.davafons.com/install.sh | sh
   Then run `parcela doctor` and tell me what it says is missing.
2. Read `parcela --skill` and https://parcela.davafons.com/docs/configuration/.
3. Find how the project runs today: Procfile, bin/dev, docker-compose.yml,
   package.json scripts, Makefile, README. Don't read .env files or secrets.
4. Write parcela.yaml with the project's real commands:
   - one service per process; HTTP services listen on 127.0.0.1:$PORT and
     have an http.healthcheck;
   - data that belongs to each environment: SQLite under $PARCELA_DATA_DIR,
     or one database per environment;
   - hooks.setup to install packages, copy .env from $PARCELA_PRIMARY_DIR and
     create and seed the database; hooks.prestart to run migrations.
   Ask me before deciding how databases, uploads or queues are shared.
5. Run `parcela doctor`, then `parcela up --detach`. Check every URL with
   curl and read `parcela logs`. Fix what fails.
6. Show me parcela.yaml and what you verified. Leave the app running.
```

When it is done, every worktree you or your agents create runs the full app with `parcela up`. Install the [Parcela skill for Claude Code](worktrees.md#set-up-a-coding-agent) so agents keep using it correctly.

The rest of this page builds a small app by hand, so you know what the agent did.

## Build it by hand

This guide takes you from nothing to a small app with a database, running at named HTTPS URLs, with a second copy of it running next to the first.

## What you'll build

You'll build a project called `hello` with:

- a static page at `https://hello.localhost:1443`,
- a JSON API at `https://api-hello.localhost:1443` that reads from a SQLite database,
- a background worker that prints a line every five seconds.

Then you'll create a Git worktree called `signup`. One `parcela up` there creates its own database and runs the whole app at `https://signup-hello.localhost:1443`, while the first copy keeps running.

The finished files are [parcela.yaml](https://github.com/davafons/parcela/blob/main/examples/docs-demo/parcela.yaml), [index.html](https://github.com/davafons/parcela/blob/main/examples/docs-demo/index.html), [db.py](https://github.com/davafons/parcela/blob/main/examples/docs-demo/db.py), [api.py](https://github.com/davafons/parcela/blob/main/examples/docs-demo/api.py) and [worker.py](https://github.com/davafons/parcela/blob/main/examples/docs-demo/worker.py).

## Assumptions

You need macOS or Linux, Python 3 and Git. The example app is plain Python, so there is nothing else to install. You don't need to know Caddy, which serves HTTPS. Parcela drives it for you.

## Install Parcela

```sh
curl -fsSL https://parcela.davafons.com/install.sh | sh
```

It installs Parcela into `~/.local/bin`, and also [Caddy](https://caddyserver.com), which serves HTTPS, unless a supported version is already on your `PATH`. `sh -s -- --help` lists its options. With Homebrew instead:

```sh
brew install davafons/tap/parcela
```

For the newest build of `main` instead of the latest release, add `--nightly`: `curl -fsSL https://parcela.davafons.com/install.sh | sh -s -- --nightly`. Its `parcela --version` includes the commit, like `0.4.0+20261020.4fed620`.

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
✓ caddy   v2.11.7 at /opt/homebrew/bin/caddy
✓ lsof    /usr/sbin/lsof
✓ ps      /bin/ps
✗ config  no parcela.yaml in /Users/you/code/hello or above; create one with `parcela init`, or pass --path to a configured checkout
✓ state   /Users/you/.local/state/parcela (private)
✓ proxy   not running; `parcela up` starts it on HTTPS on :1443
· HTTPS trust is not checked, and only `parcela trust` changes it. If the browser warns, run `parcela trust` once.

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
# yaml-language-server: $schema=https://parcela.davafons.com/schema/v1.json
# Reference: https://parcela.davafons.com/docs/configuration/
version: 1
project: hello

services:
  web:
    # No dev server detected; this serves the directory, replace it with yours
    command: python3 -m http.server $PORT --bind 127.0.0.1
    http:
      healthcheck: /

# Commands that prepare a new environment, such as installing packages or creating a database:
# hooks:
#   setup:
#     command: bin/setup
```

`web` is a service. `command` starts it, and Parcela passes it a free port in `$PORT`. `http` gives it a URL, and `up` waits until `GET /` answers before opening it. The project is named after the directory.

## Start it

```sh
parcela up
```

```text
parcela: started the proxy on :1443; run `parcela trust` to trust https://hello.localhost:1443
web | 127.0.0.1 - - [17/Oct/2026 20:40:13] "GET / HTTP/1.1" 200 -
✓ hello is running at https://hello.localhost:1443
  Press Ctrl-C to stop
```

The first time, `up` also starts Parcela's HTTPS proxy in the background. The proxy keeps running for every project until you run `parcela proxy stop`.

Open `https://hello.localhost:1443` in your browser. You see "Hello from Parcela". The first `up` in a terminal also asks for your password to trust Parcela's certificate authority. If you skipped that, the browser warns about the certificate first; the next section covers it.

Press Ctrl-C to stop the app.

## Trust the certificate

Parcela's proxy signs certificates with its own local certificate authority. Tell your system to trust it once:

```sh
parcela trust
```

```text
Adding Parcela's certificate authority to the system trust store. It may ask for your password.
  certificate  /Users/you/.local/state/parcela/proxy/data/pki/authorities/local/root.crt
  undo         caddy untrust --cert /Users/you/.local/state/parcela/proxy/data/pki/authorities/local/root.crt
Trusted. Restart the browser if it still warns.
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
  PROCESS  STATE    PID
  web      running  70840
  path /Users/you/code/hello
```

`primary` is the name of this environment. The main checkout of a project is `primary` unless you name it.

## Add an API with a database

Create `db.py`. `setup` creates a table with one message, and `list` prints the messages:

```python
import os
import sqlite3
import sys

database = sqlite3.connect(os.environ["DATABASE_PATH"])

if sys.argv[1] == "setup":
    database.execute("CREATE TABLE IF NOT EXISTS messages (text TEXT)")
    database.execute("INSERT INTO messages VALUES (?)", (f"hello from {os.environ['PARCELA_ENVIRONMENT_NAME']}",))
    database.commit()
    print("database ready")
else:
    for (text,) in database.execute("SELECT text FROM messages"):
        print(text)
```

Create `api.py`. It answers every request with the message and its own URL:

```python
import json
import os
import sqlite3
from http.server import BaseHTTPRequestHandler, HTTPServer


class Hello(BaseHTTPRequestHandler):
    def do_GET(self):
        database = sqlite3.connect(os.environ["DATABASE_PATH"])
        (message,) = database.execute("SELECT text FROM messages LIMIT 1").fetchone()
        body = {"message": message, "url": os.environ["PARCELA_SERVICE_API_URL"]}
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(body).encode() + b"\n")


HTTPServer(("127.0.0.1", int(os.environ["PORT"])), Hello).serve_forever()
```

Replace `parcela.yaml` with:

```yaml
version: 1
project: hello

env:
  DATABASE_PATH: ${PARCELA_DATA_DIR}/hello.sqlite3

services:
  web:
    command: python3 -m http.server $PORT --bind 127.0.0.1
    http:
      healthcheck: /
  api:
    command: python3 api.py
    http:
      subdomain: api
      healthcheck: /

hooks:
  setup:
    command: python3 db.py setup
```

Three things are new:

- `env` gives every command a `DATABASE_PATH` inside `PARCELA_DATA_DIR`, a directory that belongs to this environment alone.
- `api` is a second HTTP service. Its `subdomain` puts it at `api-` plus the environment's hostname.
- `hooks.setup` runs once, the first time an environment starts. It creates the database before any service starts.

Restart the environment to pick up the new configuration:

```sh
parcela restart --detach
```

```text
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
{"message": "hello from primary", "url": "https://api-hello.localhost:1443"}
```

`primary` is this environment's name, from `PARCELA_ENVIRONMENT_NAME`. The API read its URL from `PARCELA_SERVICE_API_URL`. Parcela sets variables like these for every service, so your services can find each other. The [configuration reference](../configuration.md#environment-variables-parcela-sets) lists them all.

See what the setup printed:

```sh
parcela logs setup
```

```text
database ready
```

## Add a worker and a task

A worker is a service without `http`: no port and no URL, like a job queue. Create `worker.py`:

```python
import time

tick = 0
while True:
    tick += 1
    print(f"tick {tick}", flush=True)
    time.sleep(5)
```

Add it under `services`, and add a task that lists the messages:

```yaml
services:
  # web and api as before
  ticker:
    command: python3 worker.py

tasks:
  messages:
    command: python3 db.py list
```

Restart again. Setup already ran in this environment, so it does not run again:

```sh
parcela restart --detach
```

Wait a few seconds and read the output of every service:

```sh
parcela logs --tail 2
```

```text
api    | 127.0.0.1 - - [18/Oct/2026 19:50:15] "GET / HTTP/1.1" 200 -
api    | 127.0.0.1 - - [18/Oct/2026 19:50:16] "GET / HTTP/1.1" 200 -
ticker | tick 1
ticker | tick 2
web    | 127.0.0.1 - - [18/Oct/2026 19:50:15] "GET / HTTP/1.1" 200 -
web    | 127.0.0.1 - - [18/Oct/2026 19:50:16] "GET / HTTP/1.1" 200 -
```

A task runs a command with the environment's variables:

```sh
parcela task messages
```

```text
hello from primary
```

## Run a second checkout in parallel

Commit the app and create a Git worktree called `signup` next to it:

```sh
git init -q && git add . && git commit -qm "hello app"
git worktree add ../signup
```

```text
Preparing worktree (new branch 'signup')
HEAD is now at 4b513a4 hello app
```

With jj, run `jj workspace add ../signup` instead. Start the app from the new checkout. This is a new environment, so `setup` runs and creates its own database:

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

The worktree belongs to the same project, `hello`, and gets its own name, `signup`. Its services, ports, logs and database are separate from the main checkout's:

```sh
curl https://api-signup-hello.localhost:1443
```

```text
{"message": "hello from signup", "url": "https://api-signup-hello.localhost:1443"}
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
✓ Stopped signup (hello)
✓ Stopped hello
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
