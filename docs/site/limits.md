# Supported scope

Parcela is experimental. These pages describe the current source interface. An older installed binary may lack service sets or `--skill`. Check the binary you actually use and coordinate writable-state upgrades with its existing owners.

## Implemented interface

| Area | Current scope |
|---|---|
| Sources | Explicit adoption of existing jj, Git or plain configured directories; read-only sibling discovery |
| Identity | Stable project/environment IDs, explicit primary or preview roles, labels and run generations |
| HTTP services | Flat named HTTPS through maintained Caddy, with all required endpoints in one native Process Compose graph |
| Processes | Foreground literal `run`, native `up`, explicit detached owners, declared finite tasks |
| Inspection | JSON context/inventory/status, native service states, bounded log tails and selected service URLs |
| Lifecycle | Selected whole-graph stop/restart and qualified native recovery without command replay |
| Agent guidance | Bundled standalone `--skill` Markdown; no automatic global profile installation |

## Verification boundaries

Disposable acceptance fixtures exercise named TLS, endpoint ownership, readiness, native graph inspection, stop/restart, qualified recovery, source-manifest snapshot behavior and listener closure on macOS and Linux. The documentation demo and agent command recipe are separate checks.

Those results do not certify a user's installed front door, every backend version, browser trust, reboot persistence or an arbitrary application's data policy. A schema check does not validate the whole native graph. An HTTP liveness path does not establish authentication, HMR, search correctness or queue behavior.

The CLI separates durable intent from fresh health. `live_health_verified` means an owner answered. An environment can retain recorded `ready` while a fresh HTTP probe reports unhealthy. Existing streams are not forcibly closed merely because a new-request route lease expires.

## Not implemented or not generally certified

- Automatic reconciliation of allocations that lack durable launch evidence.
- Takeover of legacy ownerless graphs, arbitrary orphan processes or replaced native handles.
- Service-specific Parcela restart, interactive attach or following logs.
- Automatic backend installation or a complete release installer.
- Alias, wildcard tenant, LAN or tailnet sharing workflows.
- Automatic database/storage isolation, baseline preparation or secret-free capture certification.
- Moving or copying environments between machines. Parcela maintains local parallel environments only.
- General daemonized-descendant containment, reboot recovery and browser trust acceptance.

Private state, locks, native authentication and process births provide operational ownership. They do not contain hostile same-user programs or make a bound-port probe an atomic listener handoff.

Preserve unsupported and failed states. Do not turn documentation, a configuration declaration or a manual cleanup into runtime proof.
