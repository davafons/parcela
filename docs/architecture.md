# Rust control plane and managed backends

## Settled direction

Parcela is a Rust binary and library. The early Go implementation drafts were
abandoned before runtime acceptance; there is no Go module, embedded Caddy,
second CLI or retained Go implementation. Caddy and Process Compose may
internally use Go; they are maintained external executables, not code compiled
into Parcela. Historical PRDs/research and arbitrary Go application fixtures are
not alternative Parcela implementations.

| Owner | Responsibility |
|---|---|
| Rust / Parcela | CLI, schema, project/checkout identity, durable registry, context, endpoint ownership, environment/task lifecycle and route reconciliation |
| Managed Caddy | HTTP proxying, TLS/certificates, WebSockets/SSE, protocol handling |
| Managed Process Compose | Service graph/dependencies/readiness, recovery/restarts, logs, control API/TUI and shutdown |
| Project configuration | Executables, preparation, data/snapshot/reset policy, resource declarations and external-effect safeguards |

The single-product experience does not require users to maintain a Caddyfile or
launch a supervisor separately. Backend acquisition/version pinning/checksums,
upgrade/rollback and standard-port/bootstrap packaging remain a technical spike.
Ordinary commands must never silently install tools, request privilege, trust a
CA or take over another user's backend/listener.

## Current crate boundaries

- `config`: strict organizer schema, native graph data, deterministic flat
  hostnames and canonical manifest/checkout discovery.
- `registry`: private SQLite state, immutable IDs, transactional adoption and
  hostname/source/primary conflicts. No ports or process handles yet.
- `context`: read-only planned URLs/private-path contract, capabilities and
  caveats. No allocation or resource preparation yet.
- `main`: argument dispatch, state resolution, JSON stdout and error stderr.

Rust dependencies include bundled SQLite, Serde and a maintained YAML parser;
`Cargo.lock` is tracked. There is no proxy stack or service scheduler dependency.

## Next runtime boundaries (not implemented)

Endpoint allocation needs persistent machine-wide leases plus actual listener
ownership verification; probe-and-release is not an atomic bind handoff. Do not
advertise a route merely because an unowned port answers. Process control needs
launch/birth identity or another validated handle; PID alone is insufficient.

Caddy uses Parcela-owned config/state and a private admin endpoint, separate from
any user Caddy. Local listeners bind loopback on 443/80 with explicit privilege
setup. Automatic trust installation is disabled; trust is a separate authorized
operation. Serialize complete owned route updates or use ETag/If-Match; unknown
hosts fail closed. Stopped preview namespaces cannot fall through to another
owner's wildcard. No fallback browser-facing port and no new handwritten proxy.

Process Compose gets an unchanged native graph plus one resolved generation of
context. Each environment has its own instance, private Unix control socket and
explicit Unix-socket mode. Rust owns environment targeting and context; Process
Compose owns scheduling, restarts, tasks where appropriate and service UI.
Readiness/restart/crash reconciliation must be experimentally verified before
advertising endpoints as ready. No interchangeable-supervisor framework or
second scheduler.

Standalone `run` will preserve literal argv, forward termination and propagate
exit status without a new automatic restart policy. Project commands remain
framework-agnostic. Data policy is never hardcoded to Rails, fresh databases or
copying development data.

Sharing, CA trust, standard-port setup, boot services and source/data destruction
are separate explicit operations. Every new capability must have disposable
acceptance fixtures and report its verified, unsupported or unknown surfaces.
