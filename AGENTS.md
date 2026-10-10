# Parcela

Workspace/environment organization and named HTTPS for arbitrary local services.
Use jj for version control; preserve unrelated work and never use mutating Git commands.

- Read docs/PRD.md and docs/implementation.md before architecture changes.
- Keep Caddy routing/TLS and Process Compose supervision as maintained backends.
- Defaults are standard HTTPS, loopback only, flat names, tenants off.
- Never take over listeners, trust a CA, expose LAN/tailnet services, or delete source/data implicitly.
- Keep project commands/data policy in configuration; no framework assumptions in core.
- Run go test ./..., go vet ./..., and scripts/check for code changes.
- Test live processes only in disposable directories with isolated state and explicit test ports.
- No GUI focus, clipboard changes, test audio, or system-wide input.
- Describe verified results and explicit unsupported/unverified functionality; docs are not runtime proof.
