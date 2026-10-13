# Worktree service sets

## Goal

One adopted worktree runs one native Process Compose graph with multiple independently allocated HTTP endpoints. Frontend and API share the environment ID, generation, owner, recovery and stop operation. Non-HTTP processes remain native graph entries. No implicit package discovery, setup, migration, reset, seed, dependency startup or data isolation.

## Workflow

- [x] Phase A: Frame. Done means actual named HTTPS reaches distinct frontend/API processes in two disposable worktrees, allocation conflicts preserve existing ownership, startup is unavailable until both services are ready, and restart/recovery/stop act on the whole graph without replay or leaked listeners. Existing single-service checks remain green.
- [x] Phase B: Design the workflow. Compare the shapes below before editing. Build a real CLI acceptance driver and capture the old refusal. Keep independently verifiable registry, runtime and integration units.
- [x] Phase C: Run the loop. Implement each unit and verify its own boundary before advancing.
- [x] Phase D: Keep the trail. Update this snapshot and /work checkpoints at unit boundaries.
- [ ] Phase E: Verify and hand back. Run required checks and real Caddy/Process Compose acceptance on macOS and Linux. Keep independent publication/install review open until authorized and completed.

## Design

Candidate A replaces all scalar session fields and rewrites persisted owner records during migration. This makes the new model uniform but cannot safely rewrite active generation-qualified records. Rejected.

Candidate B keeps the default backend_port/public_url as compatibility fields and adds a typed endpoint vector for new runs. Legacy records with no vector resolve to the manifest's default service. Schema 3 adds explicit hostname reservations and active-port reservations. All leases are claimed in one immediate transaction and released only on qualified stopped completion. Selected. Read-only schema 1/2 remains supported without migration. New writable opening migrates the private registry, never owner records. Old binaries cannot operate schema 3, so the installed registry must not be migrated before an explicit upgrade of its proxy and owners.

Candidate C creates an environment per HTTP service. Rejected because it violates the requested worktree-owned service set and fractures graph recovery/stop.

Secondary names are flat prefixes: api-nadeshiko.localhost and api-review-nadeshiko.localhost. The existing subdomain field supplies the prefix. Adoption reserves all declared hosts, including conflicts with another worktree's base name. Changing reserved service names requires stopped state and explicit re-adoption. Native graph entries supply service commands, working directories and per-process PORT overrides. Generated PARCELA_SERVICE_<UPPERCASE_NAME>_PORT, _URL and _HOSTNAME bindings feed native graph interpolation. PORT/PARCELA_URL remain the default-service aliases.

All declared HTTP services are required for initial and re-established group readiness. Each listener must belong to the supervised tree. Changed listener identity withdraws the group and requires readiness again. Status probes every endpoint and exposes per-service observations. A single generation lease gates all group routes. Stop withdraws all routes before native graph shutdown. Recovery uses the immutable recorded manifest and endpoint collection.

## Throughput checkpoint

- Blocking first steps. Inspect manifest, registry, runtime, owner, router, control and context; choose legacy compatibility and atomic reservation rules before edits.
- Independent workstreams. No code delegation authorized. Static checks and external disposable host checks can run independently only after the coupled unit builds.
- Shared mutable state. One owner edits core state and this plan. No installed registry, primary application or unrelated sibling writes.
- Smallest safe decomposition. Serial registry/reservation, runtime/routing, then integration units. The session-generation invariant crosses these modules and rules out independent editing lanes.

## Current state

Core implementation and parent verification complete atop published bd106c06. Data shape is Session.endpoints plus typed EndpointHealth. Schema 3 atomically reserves all hosts/ports and retains legacy default aliases. Both hosts pass 77 Rust and 20 Python checks, eleven real service-set cases, normal native acceptance and 30 recovery cases. Local final evidence is ~/.local/state/parcela/agent-tests/service-sets-20261013/mac-final.json and recovery-mac.json. Linux evidence is service-sets-final.json and recovery.json in that local directory, mirrored from /tmp/parcela-service-sets-20261013 on Tower. A native legacy-process-name regression failed before restoring single-service compatibility. Native listener turnover exposed a stale lsof observation; process birth/liveness checks fix it without accepting a stable foreign listener.

One earlier failed fixture /private/tmp/pc-set-2oqor2q3 left a backend orphan on 127.0.0.1:57831. Native supervisor exited, so no authenticated stop handle survives. Do not signal a persisted PID. Question q-64764d3e868448c3 requests explicit qualified cleanup after fresh live birth/command/environment checks. No answer yet. All final fixtures closed their listeners; that does not certify this earlier failed fixture.

Nadeshiko has five additive files only: parcela.yaml, scripts/parcela-dev, scripts/parcela-service, scripts/parcela-dev.test.mjs and docs/parcela-development.md. Six fake-executable tests, Biome, ShellCheck, schema validation and real native dry-run pass. No existing source, npm scripts, generated files or private-upload edits changed. Service wrappers use native working directories, explicit loopback, separate backend PORT, system CA use and IPv4-first DNS. Migrations are forced off and preview launch refuses without explicit data policy. Normal PgBoss work remains allowed. No application or task ran.

Final serial diff, comment and style review completed. Exact-source static checks and the eleven-case native service-set acceptance pass on both hosts. Independent review remains required before publication or installation. Docker still reports missing OrbStack socket; configured PG5432 and ES9200 have no listeners. Standard-port forwarding and installed binary cutover remain unverified and separate. No host/PF/CA/primary/data changes. No code delegation, external publication or sibling history restructuring.
