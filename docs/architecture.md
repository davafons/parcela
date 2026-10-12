# Rust control plane and managed backends

Parcela owns identity, context and integration, not a proxy stack or a second supervisor. There is no retained Go implementation. Caddy and Process Compose are external maintained executables.

| Owner | Responsibility |
|---|---|
| Rust | Schema, registry, roles/labels, endpoint intent, locks, literal argv, task results and route updates |
| Caddy | HTTP/TLS, certificates, HTTP protocol handling, streaming and upgrades |
| Process Compose | Native graph, scheduling, workers, control API, logs and graceful shutdown |
| Project | Commands, framework integration, preparation/data baseline, resource paths and external-effect policy |

## Crate boundaries

`config` validates organizer fields and retains native graph data. `registry` serializes durable identity and session/port intent, with generation/nonce fencing. `context` describes registered/planned metadata without allocating resources or certifying live health. `private` owns private files/kernel locks. `process` retains foreground child handles/groups and verifies listener PID ancestry. `http` is a bounded control/readiness client, never a proxy. `router` generates and updates the dedicated Caddy configuration. `control` provides bounded generation-bound messages to the live owner. `lifecycle` handles CLI-side stop/detach/status. `discovery` queries read-only VCS inventory. `runtime` supplies context and monitors one routed endpoint plus native workers. `main` dispatches CLI/JSON and propagates results.

## Ownership

Adoption is explicit; a preview never implicitly becomes primary. Branches/bookmarks are not identities. Missing source and uncertain runtime state do not authorize deletion. Inspection does not create absent state. Private user-owned paths are operational boundaries, not isolation against malicious same-user code.

A bound port probe plus durable lease is not an atomic listener handoff. Before publishing, the runtime verifies that the loopback listener belongs to its live retained child/descendants and passes HTTP readiness. Changed listener identity withdraws the route and repeats readiness. `status` probes HTTP health on demand; the runtime does not continuously poll it after establishment. Crash recovery from stored PIDs is unsupported.

## Dedicated routing

Caddy uses isolated config/storage and a private Unix admin socket, separate from any user's Caddy. Updates serialize complete configs and verify the current ownership marker. Ready sessions route only under their router token. Registered unavailable names retain exact unavailable routes and TLS subjects; unknown hosts cannot fall through to another environment. Certificates are internal; automatic trust installation is disabled.

Standard ports are the normal browser-facing plane. Tests explicitly choose high ports. An external standard-port forwarding profile explicitly separates unprivileged internal 8080/8443 from public 80/443; it preserves canonical URLs and checks the public port-80 marker before application startup. The macOS setup helper is separately authorized, checks existing rules/listeners, and installs only its own guarded forwarding anchor/boot job and specified CA trust. Caddy and application commands stay unprivileged. Host setup evidence and unverified reboot/browser behavior are tracked in [implementation status](implementation.md).

## Supervision and teardown

Native Process Compose data is exported without organizer schema reinterpretation, with resolved context and explicit private Unix-socket mode. Projects can include non-routed workers. Its own scheduler/control API remains authoritative for service state. Each application owner retains its foreground child handle and a private generation-bound `ctl.sock`. `up --detach` launches an independent Parcela owner, verifies its live readiness and direct-child identity, then releases the caller. Process Compose still owns scheduling and worker recovery. An explicitly installed launch agent may host the foreground front-door command.

On normal application teardown, core marks stopping, revokes routes, signals the retained child/group, then marks stopped. Failed revocation reports failure and retains the lease. Unknown existing sockets or interrupted leases fail closed; no implicit deletion, takeover or persisted-PID kill. Children must stay supervised and honor shutdown: detached/daemonized descendants are not certified.

Standalone `run` preserves literal argv and has no automatic restart policy. Project tasks are optional finite argv with explicit timeouts/results. `up` does not prepare, reset or import data. The Shirabe bridge belongs to that app, not Rust: it authorizes hosts, supplies canonical HTTPS URLs, scopes cookies and chooses primary versus preview paths/secrets.

Backend acquisition/upgrade packaging, robust crash reconciliation, interactive attach, service-specific restart, sharing and broader release acceptance remain future work. Older owners without the private control protocol are not adopted or stopped implicitly. Ordinary commands never install software, request privilege, trust a CA or take over listeners.
