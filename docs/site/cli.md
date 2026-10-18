# CLI guide

Every Parcela command acts on one environment: the checkout in the current directory, unless you pick another one. This page groups the commands by what you want to do. Run `parcela <command> --help` for the full flag list.

## Pick the environment

These flags work with every command:

| Flag | Meaning |
|---|---|
| `--path DIR` | Act on the checkout at `DIR` instead of the current directory. Parcela uses the nearest `parcela.yaml` at or above it. |
| `--workspace ID` | Act on a registered environment by its ID. Get IDs from `parcela ls --json`. Conflicts with `--path`. |
| `--state-dir DIR` | Use another state directory. The default is `PARCELA_STATE_DIR`, then `$XDG_STATE_HOME/parcela`, then `~/.local/state/parcela`. |
| `--json` | Print JSON, even in a terminal. |

`parcela` with no command shows the current checkout's status, or the next command to run:

```text
$ parcela
No parcela.yaml here. Create one with parcela init, then run parcela up.
See parcela --help for every command.
```

## Text and JSON output

In a terminal, commands print aligned text. When stdout is a pipe, or with `--json`, they print one JSON document. Parse the JSON, not the text:

```sh
parcela status --json | jq -r .state
```

```text
ready
```

`url` and `logs` always print plain text, so `curl "$(parcela url)"` works. `task` and `run` pass their command's output through unchanged. Errors go to stderr and start with `parcela:`. Colors appear only in a terminal, and `NO_COLOR` turns them off.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success. |
| 1 | The command failed, `doctor` found a problem, or `proxy status` found no proxy. |
| 2 | Bad arguments. |
| 124 | A task ran past its `timeout_seconds`. |
| other | `task` and `run` exit with the code of the command they ran. |

## Start and stop

### parcela init

```text
parcela init [--name NAME] [--command CMD]
```

Writes a one-line `parcela.yaml` in the current directory. It guesses the web command from `package.json`, a Rails app or Django, and serves the directory with Python otherwise. `--command` sets the command. `--name` sets the project name. `init` never overwrites an existing file.

```text
$ parcela init
✓ Created /Users/you/code/hello/parcela.yaml
  web  python3 -m http.server $PORT --bind 127.0.0.1
  url  https://hello.localhost:1443

Next: parcela up
```

### parcela up

```text
parcela up [--detach] [--name NAME | --primary] [--backend-port N]
```

Starts every service and worker in `parcela.yaml` and waits until each service answers. It registers the checkout the first time, and starts the proxy if it is not running. If the services kept running after Parcela was interrupted, `up` takes control of them again instead of starting a second copy.

- Without `--detach`, `up` stays in the foreground and prints the processes' output. Ctrl-C stops everything.
- `--detach` returns once the services are ready and leaves them running.
- `--name NAME` names this checkout on its first `up`. `--primary` gives a linked checkout the project URL.
- `--backend-port N` sets the default service's port. Other ports are picked for you.

```text
$ parcela up --detach
✓ hello is running at https://hello.localhost:1443
  api       https://api-hello.localhost:1443

  parcela logs    show service output
  parcela status  check health
  parcela down    stop it
```

```text
$ parcela up --name review --detach
✓ review (hello) is running at https://review-hello.localhost:1443
  api       https://api-review-hello.localhost:1443
```

### parcela down

```text
parcela down [--all | --all-projects]
```

Stops the environment's services and workers. `stop` is the same command. Source, data and the environment's URLs stay. Running `down` again does nothing.

- `--all` stops every running environment of this project.
- `--all-projects` stops every running environment on the machine.

```text
$ parcela down
✓ Stopped hello; source and data preserved
$ parcela down
✓ hello was already stopped
```

### parcela restart

```text
parcela restart [--detach] [--backend-port N]
```

Stops everything and starts it again from the current `parcela.yaml`. It checks that the new configuration can start before it stops the running one. Use it after you change a command, a worker, `ports` or `environment`. A new or renamed service needs `down` and then `up`, because it needs a new hostname.

## Inspect

### parcela status

```text
parcela status
```

Shows whether the environment is running and checks it live: each service's HTTP health and URL, each worker's process state, and the named ports.

```text
$ parcela status
hello primary  ready
  https://hello.localhost:1443
  SERVICE  HTTP   URL
  api      ready  https://api-hello.localhost:1443
  web      ready  https://hello.localhost:1443
  PROCESS  STATE    PID
  api      running  53311
  ticker   running  53308
  web      running  53318
  db        21565  PARCELA_PORT_DB
  path /Users/you/code/hello
```

The state is one of `ready`, `starting`, `unhealthy`, `stopped`, `interrupted` or `unknown`. [Troubleshooting](troubleshooting.md) says what to do for each.

### parcela ls

```text
parcela ls
```

Lists every environment on the machine, across projects. It also lists sibling jj workspaces and Git worktrees of the current project that have not been started yet.

```text
$ parcela ls
PROJECT  NAME     STATE        URL                                   PATH
hello    signup   ready        https://signup-hello.localhost:1443   /Users/you/code/signup
hello    primary  ready        https://hello.localhost:1443          /Users/you/code/hello
hello    billing  not started  https://billing-hello.localhost:1443  /Users/you/code/billing
```

### parcela logs

```text
parcela logs [SERVICE] [--tail N]
```

Prints the last lines of every service and worker, each line prefixed with its name. Name a service or worker to see only its output. `--tail` is the number of lines per process, 100 by default. It does not follow the log.

```text
$ parcela logs --tail 2
api    | 127.0.0.1 - - [18/Oct/2026 19:06:52] "GET / HTTP/1.1" 200 -
api    | 127.0.0.1 - - [18/Oct/2026 19:06:53] "GET / HTTP/1.1" 200 -
ticker | tick 1
ticker | tick 2
web    | 127.0.0.1 - - [18/Oct/2026 19:06:52] "GET / HTTP/1.1" 200 -

$ parcela logs ticker --tail 2
tick 2
tick 3
```

### parcela url

```text
parcela url [--service NAME]
```

Prints the URL of the default service, or of `NAME`. It works while the environment is stopped too.

```text
$ parcela url --service api
https://api-hello.localhost:1443
```

### parcela doctor

```text
parcela doctor
```

Checks Process Compose, Caddy, `lsof`, `ps`, the nearest `parcela.yaml`, the state directory and the proxy. It changes nothing. Every failed check says how to fix it, and the command exits 1 until all pass. It also validates `parcela.yaml`, so run it after you edit the file.

```text
$ parcela doctor
✓ process-compose  v1.122.0 at /opt/homebrew/bin/process-compose
✓ caddy            v2.11.7 at /opt/homebrew/bin/caddy
✓ lsof             /usr/sbin/lsof
✓ ps               /bin/ps
✓ config           /Users/you/code/hello/parcela.yaml (project hello)
✓ state            /Users/you/.local/state/parcela (private)
✓ proxy            not running; parcela up starts it on HTTPS on :1443
· HTTPS trust is not checked, and only parcela trust changes it. If the browser warns, run parcela trust once.

Next step: parcela up
```

With `--json` it prints `{"ready": bool, "checks": [{"name", "ok", "detail", "fix"}], "notes": [...]}`.

## Proxy and HTTPS

### parcela proxy

```text
parcela proxy start [--port N]
parcela proxy stop
parcela proxy restart [--port N]
parcela proxy status
```

The proxy is one background Caddy that serves the URLs of every environment. `up` starts it when it is not running, so you rarely need these commands.

- `start --port N` listens on port `N` and saves it for later starts. The default is 1443, or `PARCELA_HTTPS_PORT` when it is set and nothing is saved. Ports below 1024 are refused.
- `stop` stops the proxy. Services keep running, but their URLs stop answering until it starts again.
- `restart` replaces the proxy. Run it after upgrading Parcela.
- `status` shows the port, process IDs, version and log file. It exits 1 when the proxy is not running.

```text
$ parcela proxy status
✓ Proxy running: HTTPS on :1443
  urls     https://<name>.localhost:1443
  pid      95029 (Caddy 95034)
  version  Parcela 0.2.0
  log      /Users/you/.local/state/parcela/proxy/proxy.log
  Run `parcela trust` once so browsers trust these URLs.
```

If the port is busy, `start` fails and names the process that holds it. It never stops that process and never picks another port.

### parcela trust

```text
parcela trust [--print]
```

Adds Parcela's local certificate authority to the system trust store, so browsers accept its URLs. It prints what it adds and how to undo it, then runs `caddy trust`, which may ask for your password. This is the only command that changes system trust.

`--print` prints the certificate's path and changes nothing. Use it with `curl`:

```sh
curl --cacert "$(parcela trust --print)" "$(parcela url)"
```

## Projects and cleanup

### parcela rm

```text
parcela rm [--prune]
```

Forgets a stopped environment and frees its URLs and ports. It refuses a running environment. Source and the runtime directory stay on disk.

`--prune` forgets every stopped environment whose checkout directory no longer exists. Run it after you delete worktrees.

```text
$ parcela rm --prune
✓ Forgot signup (hello); released api-signup-hello.localhost, signup-hello.localhost
  data kept in /Users/you/.local/state/parcela/environments/c8af904e-1dff-495a-b4ce-0b163c9b2077
```

## Tasks and one-off commands

### parcela task

```text
parcela task NAME [-- ARGS...]
```

Runs a task from the `tasks` section of `parcela.yaml` with the environment's variables: its ports, URLs, `DATABASE_URL` and so on. Arguments after `--` are appended to the task's command. The environment must have been started at least once. It can be stopped.

```text
$ parcela task psql -- -tAc 'show port'
26341
```

### parcela run

```text
parcela run [--name NAME | --primary] [--backend-port N] -- COMMAND...
```

Runs one command in the foreground as the project's only web service, instead of the services in `parcela.yaml`. The command must listen on `127.0.0.1:$PORT`. `run` passes the command through as-is, with no shell, so wrap it in `sh -c` to use variables. It works only in projects with a single service.

```text
$ parcela run -- sh -c 'exec python3 -m http.server "$PORT" --bind 127.0.0.1'
Serving HTTP on 127.0.0.1 port 23225 (http://127.0.0.1:23225/) ...
parcela: ready https://solo.localhost:1443 (generation 2, routing-only; data isolation requires project bindings)
```

## Agents

### parcela skill

```text
parcela skill
parcela --skill
```

Prints the agent instructions bundled with this binary as Markdown. It reads no project or state, so it works anywhere. [Parallel worktrees and agents](worktrees.md#set-up-a-coding-agent) shows how to install it for Claude Code.
