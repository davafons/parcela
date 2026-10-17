# Diagnose and recover an environment

## Collect the selected environment's state

```sh
parcela --workspace "$ENVIRONMENT_ID" status
parcela --workspace "$ENVIRONMENT_ID" services
parcela --workspace "$ENVIRONMENT_ID" logs backend --tail 100
```

Use a native process name from `services`. Preserve the diagnostic without publishing secrets. `services` and `logs` require a running Process Compose environment; they are not arbitrary orphan inspectors.

| Observation | Next check |
|---|---|
| `starting` | Assigned ports, loopback binding, readiness path and child diagnostics |
| `unhealthy` with recorded `ready` | Fresh endpoint health, front-door ownership and route publication |
| `unknown` | Owner/control evidence and diagnostic; do not guess a PID |
| `interrupted` | Qualified recovery or an explicit operator plan; retain the lease |
| `stopped` | A new explicit start if requested, not recovery of running services |

## Front-door or certificate failure

Run `parcela proxy status` before modifying the host. It reports the mode, port, process IDs, log path and the Parcela version that started the proxy. Read `<state>/proxy/proxy.log` for the proxy's own errors.

| Observation | Next check |
|---|---|
| `port N is already in use by ...` | Stop that process yourself, or choose a free port with `parcela proxy start --port <n>` |
| `the running proxy is from Parcela X` | `parcela proxy restart` once nothing depends on the old proxy's routes |
| `started by an older Parcela` | Stop that `parcela proxy run` (Ctrl-C or its launch agent), then `parcela proxy start` |
| URLs answer `503` | The environment is registered but not ready; check `parcela status` |
| Browser certificate warning | `parcela trust`, then restart the browser |

In standard mode, internal listeners do not prove effective 443/80 forwarding. A successful PF rule load does not prove that loopback traffic reaches them. Do not flush global firewall state or change a skip setting blindly.

If TLS trust fails, identify the exact CA and hostname before an explicit trust operation. `parcela trust --print` names the certificate. Do not disable verification. If `.localhost` resolves to `::1` while the owned front door listens on IPv4, configure the project's client address-family behavior without exposing a new listener or weakening TLS.

A disposable test proxy on another port can isolate backend behavior. It does not fix or certify the installed front door. See [the front door](install.md#the-front-door).

## Backend port or readiness failure

Check that the app consumes the allocated context and listens only on `127.0.0.1`. `--backend-port` affects the default service only. Other services need their own generated port binding.

Check each `readiness_path`. A fast 2xx response and owned listener are required for initial group routing. Verify intended dependencies separately. Do not run migrations or preparation merely because startup failed.

A foreign or reserved port is a refusal, not a request to kill its listener. Restart preflight checks explicit replacement-port eligibility before stopping a healthy owner, but concurrent restart is not an atomic transaction across the host.

## Recover a surviving native graph

Only when the controller has died and the selected recovery is authorized:

```sh
parcela --workspace "$ENVIRONMENT_ID" recover --detach
parcela --workspace "$ENVIRONMENT_ID" status
```

Recovery requires matching generation, process births, original lock/socket identities and native authentication. It uses the recorded launch manifest instead of replaying startup commands from edited source.

Recovery can finish as `stopped` when stopping intent or a quiescent stopped receipt already exists. Do not insist on a ready response for a pending stop. `down` can also use qualified recovery when its live controller is gone.

If the evidence is missing or replaced, retain the record and diagnostic. Do not launch a duplicate graph, signal a saved PID, remove a socket or release the lease manually. Automatic recovery without launch evidence is not supported.

## Incomplete stop

`down` withdraws routing before native shutdown. It waits for qualified completion, within the recorded shutdown budget. An interrupted result does not prove every descendant exited.

Retry only the same selected operation or follow a qualified recovery plan. Do not stop another environment to resolve contention. A controller request from an old generation cannot act on a successor.

Manual operator cleanup, when specifically authorized and revalidated, is separate from a native stopped receipt. Preserve evidence and do not rewrite a failed run into a successful recovery claim.
