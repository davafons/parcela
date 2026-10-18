---
name: parcela
description: "Use Parcela to inspect, start, stop, restart or recover named local development environments, retrieve service URLs and logs, and run explicitly configured project tasks. Use for Parcela operations, not terminal layout, VCS history or implicit application setup."
---

# Parcela

Parcela manages an explicit service set belonging to one configured checkout. Caddy owns HTTPS routing. Process Compose owns application processes and workers. Project configuration owns commands, working directories, dependencies, writable resources and external effects.

This skill does not grant permission to start services, process queues, run migrations, change trust or expose a listener. Follow the user's authorization and the project's instructions. Do not print credentials, `.env` contents, native tokens or full process environments.

## Learn and select the target

Use the same binary and registry for discovery and subsequent operations. Check unfamiliar syntax with `parcela --help` or the relevant command's `--help`. `parcela --skill` and `parcela skill` print this bundled document without reading project or registry state.

Output is JSON when stdout is not a terminal and aligned text on a terminal. Pass `--json` whenever you parse a result. `url` and `logs` always print plain text.

Set `CHECKOUT` to the intended existing checkout's absolute path. Do not infer the target from a focused terminal, bookmark or branch name.

```sh
parcela --json ls
parcela --json --path "$CHECKOUT" doctor
parcela --path "$CHECKOUT" status
```

`ls` returns `.workspaces`, each containing `.workspace.id`, `.workspace.source`, `.workspace.role` and live status. Run inside a project, it also returns `.unregistered`: that project's jj workspaces and Git worktrees that were never started, with `state` `not started` and the `url` `up` would give them. Listing them registers nothing. `status` requires a registered configured checkout and returns its `.workspace` identity, `.url` and live state. Read its `parcela.yaml` and project instructions before a mutation. Schema validation and planned URLs do not prove readiness or data isolation. `doctor` returns `{ready, checks: [{name, ok, detail, fix}], notes}` and exits 1 until ready. Its `config` check reports the same validation error `up` would print. Report failed checks and their `fix`; do not apply a fix that installs tools or changes trust without authorization.

If the user asks to configure a checkout that has no `parcela.yaml`, `parcela init` writes a minimal version 1 `parcela.yaml` with a guessed command. Review the guess with the user. `init` never overwrites an existing file.

For a registered target, read its UUID from the response. Prefer that exact ID when issuing later operations from a different directory:

```sh
parcela --workspace "$ENVIRONMENT_ID" status
```

`--path` and `--workspace` conflict. Keep an explicit `--state-dir` prefix on every call if using a nondefault registry. UUID targeting does not select another machine. Missing source paths do not authorize source or registry deletion.

For sibling checkouts, read `ls` `.unregistered`. Read the selected checkout's project instructions, `parcela.yaml` and data bindings before startup. `up` registers only the selected checkout automatically when startup is authorized. It does not create worktrees or change VCS history.

On first startup, main jj/Git checkouts and plain directories use the project base URL. Linked jj workspaces use their workspace name; linked Git worktrees use their directory name. Use `--name NAME` for an explicit preview name. Use `--primary` only when the user intends that checkout to own the base URL. Unknown or conflicting identities fail rather than taking another environment's namespace. Existing registered names and roles remain stable after VCS metadata changes.

A preview name and distinct URL do not isolate databases, uploads, secrets or queues. Refuse startup until the project's bindings and external-effect policy are appropriate.

## Start and verify

Check the environment before startup:

```sh
parcela --path "$CHECKOUT" status
```

An authorized `up`, `restart` or `run` starts Parcela's shared HTTPS proxy when none runs and says so in one stderr line. `parcela proxy status` reports it without starting anything. The proxy serves every environment on the machine: do not stop or restart it unless the user asks. URLs carry the proxy port (`:1443` by default) until standard-port forwarding is set up, so always use the URLs Parcela returns. For TLS checks without changing system trust, use `curl --cacert "$(parcela trust --print)"`. `parcela trust` changes system trust; run it only when the user asks.

If already ready, reuse the environment unless the user requests a restart. If prerequisites are absent, report them. Do not automatically install backends, start Docker or a GUI, reuse another agent's dependencies, repair forwarding, trust a CA, change global firewall rules or take over listeners. Do not open an installed registry for writing with a newer binary while an older proxy or controller still serves it; an upgrade requires an explicit coordinated plan.

After authorized startup, use detached mode for agent operations:

```sh
parcela --path "$CHECKOUT" up --detach
parcela --path "$CHECKOUT" status
parcela --path "$CHECKOUT" url
parcela --path "$CHECKOUT" url --service backend
```

Use a declared service name instead of assuming every project has `backend`. `up --name review --detach` selects a preview name on first startup. `up` starts the whole native graph. `run -- ARGV` is a literal foreground command for a single HTTP service, not a substitute for a service-set graph. `--backend-port` selects the default service only. Do not assume a fixed backend port.

Each run has one generation and nonce with a recorded endpoint set. All declared HTTP endpoints must have graph-owned loopback listeners and pass initial readiness before the group routes open. Services without `http` get no route. `working_dir`, `env` and `depends_on` are per-service fields in `parcela.yaml`.

Inspect `status.state`, `status.live.endpoints` and `status.processes`, where each Process Compose service and worker has a `name`, lowercase `state` and `pid`. `live_health_verified` means the owner answered, not that every HTTP probe succeeded. Recorded `ready`, planned URLs and native `Running` states are not interchangeable with fresh HTTP health. Check the intended application path over its returned HTTPS origin when the task needs application proof. Never disable TLS verification to make a check pass.

Report the selected environment ID, role, generation, returned URLs, observed health and unverified behavior. Leave user-managed environments running unless stop was requested.

## Diagnose without guessing ownership

```sh
parcela --workspace "$ENVIRONMENT_ID" status
parcela --workspace "$ENVIRONMENT_ID" logs backend --tail 100
```

`logs` without a name prints every process, each line prefixed with `name |`. Process names are in `status.processes`. Logs are bounded tails; `logs --follow` streams until interrupted, so do not use it in a command that must return. Avoid copying secrets from diagnostics.

`unknown`, `interrupted`, missing evidence or a replaced native handle is not permission to clear locks, delete sockets, free leases or kill a process. Never signal a persisted PID, use broad `pkill`, or bypass the owner through raw native commands. Preserve the diagnostic and ask for an explicit recovery or manual intervention plan.

When the owner has died while its services still run, and the task authorizes starting the environment, `up` takes them back instead of starting a second graph. It prints `taking back control of its running services` on stderr and returns `recovered: true`:

```sh
parcela --workspace "$ENVIRONMENT_ID" up --detach
parcela --workspace "$ENVIRONMENT_ID" status
```

Recovery requires matching generation, process births, original lock/socket identities and native authentication. It uses the recorded launch manifest and does not replay startup. `up` reattaches only to a run that was starting or ready. A run that was stopping or interrupted still needs `parcela down` or the hidden `parcela recover`; a pending stop remains a stop, so that result can be `stopped` rather than `ready`. Do not retry by launching a duplicate graph when recovery refuses.

## Restart, stop and optional project tasks

Only for the explicitly selected authorized environment:

```sh
parcela --workspace "$ENVIRONMENT_ID" restart --detach
parcela --workspace "$ENVIRONMENT_ID" status
parcela --workspace "$ENVIRONMENT_ID" down
parcela --workspace "$ENVIRONMENT_ID" status
```

Restart preflights the replacement and starts the whole graph in a new generation. It does not restart one named service or replay arbitrary `run` commands. Stop withdraws routes before native shutdown and preserves source/data. Do not stop other environments as test cleanup. Do not treat a refused or incomplete stop as successful cleanup. `down --all` stops every running environment of the project and `--all-projects` every one; use them only when the user asks for that scope.

`rm` forgets a stopped environment's registration and releases its hostnames; `rm --prune` does so for every stopped registration whose checkout is gone. Both keep source and runtime data, refuse running environments, and are only for explicit cleanup requests.

Tasks are optional; ordinary project commands remain usable directly. Before `task NAME -- ARGS`, read that task's command, timeout and effects in `parcela.yaml`. Run only the requested task with literal arguments.

`up` runs the project's own `hooks.setup` on an environment's first start and `hooks.prestart` on every start, before the other services. Read both in `parcela.yaml` before the first `up` and tell the user what they will do, such as installing packages, creating a database or migrating. `up --setup` reruns setup; use it only when asked. A failed hook stops `up` and prints its output. Do not run preparation commands that `parcela.yaml` does not declare.

The default service uses the environment base hostname. Other HTTP services use a flat prefix from `subdomain` or the service name. For a preview labeled `review` of `my-app`, typical names are `review-my-app.localhost` and `api-review-my-app.localhost`. Read actual reserved or recorded URLs instead of constructing them. Hostname reservations remain while stopped; changing service reservations requires a stopped environment and a new `up` with the existing identity.

## Limits

This is an experimental local runtime manager, not a sandbox, terminal multiplexer or cross-machine environment transfer tool. It does not automatically reconcile allocations missing launch evidence, adopt arbitrary orphan processes, restart individual services, create data replicas or certify reboot/browser trust. A skill, command help or configuration document is not runtime proof.
