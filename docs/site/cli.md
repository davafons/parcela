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
No parcela.yaml here. Create one with `parcela init`, then run `parcela up`.
See `parcela --help` for every command.
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

Writes a minimal `parcela.yaml` in the current directory. It guesses the web command from `package.json`, a Rails app or Django, and serves the directory with Python otherwise. `--command` sets the command. `--name` sets the project name. `init` never overwrites an existing file.

```text
$ parcela init
✓ Created /Users/you/code/hello/parcela.yaml
  web  python3 -m http.server $PORT --bind 127.0.0.1
  url  https://hello.localhost:1443

Next: parcela up
```

### parcela up

```text
parcela up [--detach] [--setup] [--name NAME | --primary] [--backend-port N] [--lan] [--lan-ip IP] [--tailscale]
```

Starts every service in `parcela.yaml` and waits until each HTTP service answers. On an environment's first `up` it runs `hooks.setup` first, and on every `up` it runs `hooks.prestart`. A hook that fails stops `up` before the other services start. It registers the checkout the first time, and starts the proxy if it is not running. On an environment that is already ready, `up` prints its URLs and exits 0. If the services kept running after Parcela was interrupted, `up` takes control of them again instead of starting a second copy.

- Without `--detach`, `up` stays in the foreground and prints each process's output, prefixed with its name. Ctrl-C stops everything.
- Until the environment is first ready, a hook that fails, an HTTP service that exits, or any process that exits nonzero stops `up` and says which one, with its exit code. A process that finishes with 0, like an asset build, is fine. After that, an exited process is reported and nothing restarts it; if an HTTP service stops listening on its port for longer than the readiness timeout, the environment stops. The timeout is `readiness_timeout_seconds` in `parcela.yaml`, 60 seconds by default.
- `--detach` returns once the services are ready and leaves them running.
- `--setup` runs `hooks.setup` again, as on a new environment.
- `--name NAME` names this checkout on its first `up`. `--primary` gives a linked checkout the project URL.
- `--backend-port N` sets the default service's port. Other ports are picked for you.
- `--lan` and `--tailscale` also serve the environment to your other devices. See [Open it on your phone or another device](#open-it-on-your-phone-or-another-device).

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

Stops the environment's services and workers: once nothing that depends on a service is still running, its `stop` command runs, if it has one, and then it gets SIGTERM, and SIGKILL after 10 seconds. If a `stop` command fails, `down` says so; run it again. `stop` is the same command. Source, data and the environment's URLs stay. Running `down` again does nothing.

- `--all` stops every running environment of this project.
- `--all-projects` stops every running environment on the machine.

```text
$ parcela down
✓ Stopped hello
$ parcela down
✓ hello is not running
```

### parcela restart

```text
parcela restart [--detach] [--setup] [--backend-port N] [--lan] [--lan-ip IP] [--tailscale]
```

Stops everything and starts it again from the current `parcela.yaml`. Use it after any change, including added, removed or renamed services. It checks that the new configuration can start before it stops the running one. When the services' URLs change, it can only reserve the new ones after the old run stops. Sharing does not carry over: pass `--lan` or `--tailscale` again.

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

The state is one of `ready`, `starting`, `unhealthy`, `stopping`, `stopped` or `unknown`. [Troubleshooting](troubleshooting.md) says what to do for each. A process is `pending`, `running`, `stopping`, `restarting`, `completed` (exited 0, like a finished hook), `exited` or `skipped` (a dependency stopped before it could start).

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
parcela logs [SERVICE] [--tail N] [--follow]
```

Prints the last lines of every service, worker and hook of the latest run, each line prefixed with its name. Name one to see only its output. `--tail` is the number of lines per process, 100 by default. `--follow`, or `-f`, keeps printing new lines until Ctrl-C or until the environment stops. The logs stay after `down`, until the next run starts, so `logs` also shows why a run stopped. Each process writes to `log/processes/<name>.log` in its runtime directory, which is cut to `<name>.log.1` past 16 MB.

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

Checks Caddy, `lsof`, `ps`, the nearest `parcela.yaml`, the state directory and the proxy. It changes nothing. Every failed check says how to fix it, and the command exits 1 until all pass. It also validates `parcela.yaml`, so run it after you edit the file.

```text
$ parcela doctor
✓ caddy   v2.11.7 at /opt/homebrew/bin/caddy
✓ lsof    /usr/sbin/lsof
✓ ps      /bin/ps
✓ config  /Users/you/code/hello/parcela.yaml (project hello)
✓ state   /Users/you/.local/state/parcela (private)
✓ proxy   not running; `parcela up` starts it on HTTPS on :1443
· HTTPS trust is not checked, and only `parcela trust` changes it. If the browser warns, run `parcela trust` once.

Next step: parcela up
```

With `--json` it prints `{"ready": bool, "checks": [{"name", "ok", "detail", "fix"}], "notes": [...]}`.

### parcela upgrade

```text
parcela upgrade [--nightly | --stable]
```

Upgrades a Parcela installed by `install.sh`: it downloads the installer and runs it again, which replaces `~/.local/bin/parcela` and updates Caddy there if needed. A nightly build stays on nightly; `--stable` switches to the latest release, `--nightly` to the newest build of main. For a Homebrew or `cargo` install it changes nothing and prints the command to run instead. Then run `parcela proxy restart`.

It is the only command that downloads anything, and only when you run it. Parcela never checks for updates.

### parcela telemetry

```text
parcela telemetry [status | enable | disable]
```

Shows or changes whether Parcela sends anonymous usage data. `DO_NOT_TRACK=1` and `PARCELA_TELEMETRY_DISABLED=1` also turn it off, and `PARCELA_TELEMETRY_DEBUG=1` prints what would be sent instead of sending it. [Telemetry](telemetry.md) lists every field.

## Proxy and HTTPS

### parcela proxy

```text
parcela proxy start [--port N]
parcela proxy stop
parcela proxy restart [--port N]
parcela proxy status
parcela proxy install
parcela proxy uninstall
```

The proxy is one background Caddy that serves the URLs of every environment. `up` starts it when it is not running, and if it dies while environments run, for example because Caddy crashed or was killed, they start it again within a few seconds. You rarely need these commands.

- `start --port N` listens on port `N` and saves it for later starts. The default is 1443, or `PARCELA_HTTPS_PORT` when it is set and nothing is saved. With `--port 443`, URLs have no port, like `https://hello.localhost`. Parcela never runs as root, so that works only where the system lets a normal user listen on 443: on Linux after `sudo sysctl net.ipv4.ip_unprivileged_port_start=443`. On macOS, use `install`.
- `stop` stops the proxy and keeps it stopped: running environments do not restart it. Services keep running, but their URLs stop answering until `proxy start` or the next `up`. It says how many environments that affects.
- `restart` replaces the proxy. Run it after upgrading Parcela.
- `status` shows the port, process IDs, version and log file. It exits 1 when the proxy is not running.
- `install`, on macOS, makes URLs portless, like `https://hello.localhost`. It asks for your password once to add the launchd service `org.parcela.https`, which owns port 443 on 127.0.0.1 and hands each connection to `nc` running as you, relaying it to the proxy's port. No Parcela code runs as root, and it survives reboots. It refuses when another program uses 443. It also takes port 80 when that is free, so `http://hello.localhost` redirects to `https://`; a busy 80 only loses that redirect. Restart running environments afterwards to pick up the new URLs. `uninstall` removes it.

```text
$ parcela proxy status
✓ Proxy running: HTTPS on :1443
  urls     https://<name>.localhost:1443
  pid      95029 (Caddy 95034)
  version  Parcela 0.4.0
  log      /Users/you/.local/state/parcela/proxy/proxy.log
  Run `parcela trust` to trust these URLs
```

The proxy answers plain HTTP on its port with a redirect to HTTPS, so `http://hello.localhost:1443` works too.

If the port is busy, `start` fails and names the process that holds it. It never stops that process and never picks another port.

### parcela trust

```text
parcela trust [--print]
```

Adds Parcela's local certificate authority to the system trust store, so browsers accept its URLs. It prints what it adds and how to undo it, then runs `caddy trust`, which may ask for your password. You need it once per machine: every project shares the one authority.

The first `up` in a terminal does this for you when the authority is not trusted yet, behind the same password prompt, and asks only once per authority. It never happens without a terminal, so agents, scripts and CI never change trust, and `PARCELA_NO_TRUST=1` turns it off.

`--print` prints the certificate's path and changes nothing. Use it with `curl`:

```sh
curl --cacert "$(parcela trust --print)" "$(parcela url)"
```

## Open it on your phone or another device

By default every URL answers on this machine only. `up` and `restart` can also serve one environment to other devices. Each flag shares that environment alone, for that run, and `down` ends it.

### On the same Wi-Fi: --lan

```text
$ parcela up --detach --lan
✓ hello is running at https://hello.localhost:1443
            https://hello.local:1443
  api       https://api-hello.localhost:1443
            https://api-hello.local:1443
```

Each service also answers at a `.local` name, which the device on your network resolves over mDNS. Parcela publishes the names with `dns-sd` on macOS or `avahi-publish` on Linux, and Caddy listens on this machine's LAN address with the same port. Devices on the network reach only the environments started with `--lan`.

The device does not trust Parcela's certificate authority, so it warns first. To stop the warning, install the certificate from `parcela trust --print` on the device. If the network changes, run `parcela restart --lan`. `--lan-ip IP` picks the address when the detected one is wrong, for example behind a VPN. `--lan` refuses a public address, so a machine with one, such as a cloud VM, cannot serve its app to the internet this way. `PARCELA_LAN=1` in the environment works like `--lan`, and `PARCELA_TAILSCALE=1` like `--tailscale`.

### On your tailnet: --tailscale

```text
$ parcela up --detach --tailscale
✓ hello is running at https://hello.localhost:1443
            https://mb.tail1234.ts.net
  api       https://api-hello.localhost:1443
            https://mb.tail1234.ts.net:8443
```

Each service gets its own HTTPS port on this machine's tailnet name, through `tailscale serve`. Tailscale's certificate is trusted everywhere, so there is no warning. Only devices on your tailnet can open it. It never uses Funnel. It needs the Tailscale CLI, a connected machine and HTTPS certificates turned on for the tailnet. Parcela picks ports that `tailscale serve status` shows free, and leaves the rest of your serve configuration alone.

### Let the dev server accept the names

Dev servers such as Vite and Rails refuse hostnames they do not know. Parcela puts the shared ones in `PARCELA_SHARE_HOSTS`, separated by commas. For Vite:

```js
server: {
  allowedHosts: (process.env.PARCELA_SHARE_HOSTS ?? '').split(',').filter(Boolean),
},
```

The publishers show up in `parcela status` and `parcela logs` as `parcela-lan-<service>` and `parcela-tailscale-<service>`. A link that the app builds from `PARCELA_URL` still points at `.localhost`, which other devices cannot open.

## Projects and cleanup

### parcela rm

```text
parcela rm [--prune]
```

Forgets a stopped environment and frees its URLs and ports. It refuses a running environment. If `parcela.yaml` has `hooks.teardown`, `rm` runs it first, and a failed teardown keeps the environment registered. Parcela itself leaves the source and the runtime directory on disk.

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

Runs one command in the foreground as the project's only web service, instead of the services in `parcela.yaml`. It still needs a `parcela.yaml` for the project name. The command must listen on `127.0.0.1:$PORT`. `run` passes the command through as-is, with no shell, so wrap it in `sh -c` to use variables. It works only in projects with a single service.

```text
$ parcela run -- sh -c 'exec python3 -m http.server "$PORT" --bind 127.0.0.1'
Serving HTTP on 127.0.0.1 port 23225 (http://127.0.0.1:23225/) ...
✓ hello is running at https://hello.localhost:1443
  Press Ctrl-C to stop
```

## Agents

### parcela skill

```text
parcela skill
parcela --skill
```

Prints the agent instructions bundled with this binary as Markdown. It reads no project or state, so it works anywhere. [Parallel worktrees and agents](worktrees.md#set-up-a-coding-agent) shows how to install it for Claude Code.
