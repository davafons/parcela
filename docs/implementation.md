# Implementation and verification

Name selected: Parcela. Implementation authorized 2026-10-10.

## Architecture choices

- Go CLI; managed Caddy binary for TLS/HTTP. Do not compile the entire Caddy dependency graph on the user's nearly-full disk.
- Process Compose is invoked as a maintained supervisor; one instance per environment with a private Unix control socket.
- One parcela.yaml per project, with a pass-through Process Compose graph and project-defined tasks.
- Adopt existing checkouts first; optional creation delegates to jj/Git and starts from main by default.
- Interactive startup may attach; unattended/agent operation has no prompts and uses explicit foreground/detached mode.
- macOS implementation and live POC here; Linux CI and cross-build do not substitute for actual Linux service acceptance.
- Shared standard-port router setup and optional trust/network sharing are explicit operations, not prerequisites silently performed in a test.

## Milestones

1. [ ] Initialize jj/GitHub, preserve and update PRD.
2. [ ] Configuration, stable identities, transactional registry, ports, context and workspace inventory.
3. [ ] Managed Caddy routes/TLS, ownership, unavailable namespaces and safe startup.
4. [ ] Standalone run and Process Compose lifecycle/tasks/logs/restart integration.
5. [ ] Shirabe Rails + SQLite disposable two-environment acceptance.
6. [ ] Optional sharing/boot/cleanup coverage and explicit capability reporting.
7. [ ] Tests, CI, documentation, commits and remote verification.

No services, trust stores, existing listeners or tailnet settings are changed by repository initialization.
