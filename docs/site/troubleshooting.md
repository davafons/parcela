# Troubleshooting

Start with these two commands. Most problems show up in one of them:

```sh
parcela doctor
parcela status
```

## The browser warns about the certificate

Your system does not trust Parcela's local certificate authority yet. Run `parcela trust` once, then restart the browser. For `curl`, pass `--cacert "$(parcela trust --print)"`. Don't use `curl -k`. It hides real certificate errors.

## The proxy port is busy

```text
parcela: port 1443 is already in use by Python (pid 57668); choose another port with `parcela proxy start --port <n>`
```

Another program listens on the port. Parcela never stops it. Stop it yourself, or pick another port with `parcela proxy start --port 4443`. Parcela saves the port for later starts.

## `up` times out

```text
parcela: application readiness timed out; see child diagnostics
parcela: services stopped before they were ready; full log: /Users/you/.local/state/parcela/environments/<id>/log/owner.log
```

A service never answered on its port. Check that its command:

- listens on `$PORT`, not a fixed port,
- binds `127.0.0.1`,
- answers `GET /` with a 2xx status, or set `readiness_path` to a path that does.

Read the log file named in the message for the service's own error.

## The URL answers 503

The environment is registered but not running, or it is still starting. Run `parcela status`. If it says `stopped`, run `parcela up --detach`.

## A preview name is taken

```text
parcela: preview name signup is already used by /Users/you/code/signup; choose another with --name <name>
```

Another checkout owns that name. Start this one with another name: `parcela up --name signup-2`. To free the name, stop the other checkout and run `parcela rm` in it.

## A checkout keeps its old name

Parcela remembers a checkout's name after the first `up`. To rename it, run `parcela down`, `parcela rm`, then `parcela up --name NEW`. The renamed checkout gets a new runtime directory, so move any files you keep in `PARCELA_RUNTIME_DIR` yourself.

## `restart` refuses a new service

```text
parcela: service hostname not reserved; explicitly re-adopt the stopped environment
```

A new or renamed service needs a new hostname, and `restart` keeps the existing ones. Run `parcela down`, then `parcela up --detach`.

## `logs` needs a running environment

```text
parcela: service inspection requires a running Process Compose environment
```

`logs` reads from the running process group. Start the environment first. Output from a failed detached start is in the `owner.log` file that the error names.

## The proxy is from another Parcela version

```text
parcela: the running proxy is from Parcela <old>, this is <new>; run `parcela proxy restart` to update it
```

You upgraded Parcela while the old proxy was running. It keeps serving. Run `parcela proxy restart` when nothing needs the URLs for a moment.

## The state is `interrupted` or `unknown`

Parcela lost contact with the process that supervises the environment, for example after a crash. Run `parcela up`. If the services are still running, `up` takes control of them again without restarting them. If they are all gone, it starts them fresh. If `up` says the run was being stopped, finish the stop with `parcela down`, then run `parcela up`.

Don't kill processes by PID or delete files in the state directory to fix this. Parcela refuses to act when it cannot prove which processes it started, and it keeps the record so nothing is lost.

## Something else

Run `parcela status --json` and read its `diagnostic` field. Report bugs at [github.com/davafons/parcela/issues](https://github.com/davafons/parcela/issues) with that output and the matching `owner.log`.
