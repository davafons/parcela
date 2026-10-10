# Multidev: conditional product requirements

Version: **0.1 — discussion draft**  
Date: **2026-10-10**  
Decision: **evaluate existing tools first; implementation not approved**

## 1. Why this draft is conditional

[Research](research.md) found multiple close implementations. A generic worktree/port/DB CLI already exists. This PRD specifies the remaining workflow requirements so they can be used both as an **evaluation checklist for existing products** and, only if needed, as a design for a new implementation.

Preferred order: **adopt → contribute adapters/fixes → thin orchestration wrapper → standalone product**. Do not rebuild process supervision, reverse proxies, package management, or container networking without a demonstrated reason.

## 2. Product thesis

> Give each agent a runnable, disposable environment for its checkout, with its own mutable state and a trustworthy verification record—without managing the agent or taking over version control.

The unit is an **environment**, not a branch, terminal, port, or worktree. A checkout contains code. An environment adds resolved configuration, services, data, endpoints, ownership, and verification artifacts.

Working name: **Multidev**. Naming/package/domain availability has not been researched.

### Primary users

- Developer running several coding agents through Herdr or another orchestrator.
- Agent working in an existing Git worktree or jj workspace.
- Reviewer opening another environment without stopping their normal dev stack.

### Main jobs

1. “Run and test my checkout without touching anyone else's environment.”
2. “Show me what is running, its code identity, its URLs, and why it is unhealthy.”
3. “Give a fresh environment the same safe baseline, then let migrations diverge.”
4. “Stop/reset/destroy only this environment, even after an interrupted operation.”
5. “Leave review evidence that identifies exactly what was tested.”

## 3. Goals and non-goals

### Goals

- macOS and Linux; native, container, and hybrid architecture.
- Stack-neutral service/task/resource model; adapters provide conventions, not core branching.
- Adopt existing checkouts first; jj and Git supported independently.
- Agent-friendly CLI: explicit targets, JSON, no prompts in unattended operation.
- Concurrency-safe allocation and recoverable lifecycle.
- Private mutable state by default; declared/visible weaker sharing modes.
- Readiness and functional verification, not merely “spawn succeeded.”
- Reuse existing project commands/toolchains and optional rb-portless routing.

### Non-goals for v1

- IDE, agent launcher, task board, merge/rebase manager, or replacement for Herdr/jj.
- Package manager or generic discovery of every dependency/secret in a repository.
- Production deployment, remote preview platform, Kubernetes orchestration.
- Security containment of arbitrary same-user native code.
- Transparent cloning of external SaaS accounts or arbitrary running DB files.
- Universal raw TCP hostname routing; protocol-aware DB proxies are not required.
- GUI automation, opening browsers automatically, modifying clipboard or trust stores silently.
- Windows support, multi-repo environments, automatic sleep/wake scheduling, plugin marketplace.

## 4. Definitions and identity

| Concept | Meaning |
|---|---|
| Project | Explicit stable project ID from its manifest; display name is separate |
| Checkout | Canonical source directory plus optional VCS provider metadata |
| Environment | Immutable random ID, human label, checkout, profile, owner lease, resource graph, generation |
| Service | Long-running arbitrary executable, possibly with multiple ports or no ports |
| Task | Finite command with dependencies, timeout, env and outputs |
| Resource | Mutable dependency with provisioning/inspection/reset/destruction lifecycle |
| Baseline | Immutable consistent data snapshot or reproducible initialization recipe |
| Endpoint | Browser/public, host-loopback, container-network or Unix-socket address |
| Verification | Task results and artifacts bound to source, manifest, toolchain and baseline identities |

Environment identity must **not** be the branch slug, bookmark name, revision hash, directory basename, or current PID. Different checkouts can share names/revisions, and jj working revisions change while editing. Human aliases are unique within a project; all backend names include an environment ID suffix and respect DNS/DB length limits.

Checkout moves require an explicit `rebind` operation after identity checks. Missing paths are not automatic permission to delete data. Two environments sharing a source path have independent runtime state but **not independent code edits**; warn and offer `fork`. Any generated writable build output must also be separated before claiming concurrent support for a shared source directory.

## 5. Isolation contract

Every environment reports its coverage and caveats. “Isolated” is not a single boolean.

| Surface | Default requirement | Explicit weaker/unsupported mode |
|---|---|---|
| Source | Separate checkout when independent edits are required | Shared path: code interference possible |
| Ports | Unique allocator lease per protocol/address; actual bind/ownership checked | No promise that arbitrary foreign processes honor registry leases |
| App processes | Owned process trees/control handles; private runtime paths | Same-user native code can escape operational boundaries |
| Database | Private instance, or separate DB/role with validated grants | Shared instance has common performance/server-failure domain |
| Cache/queue | Private Redis/queue instance for destructive verification | Prefix/Redis DB number explicitly marked weaker isolation |
| Files | Private tmp/PIDs/logs/uploads/cache/build outputs | Shared writable bind mounts require explicit opt-in |
| Dependencies | Locked reinstall, immutable shared cache, or safe CoW copy | Symlinked writable dependency/build trees are not safe default |
| Browser | Per-environment hostname; host-only/environment-scoped cookies | Parent-domain cookies can cross environments |
| External effects | Mocks or approved environment-specific endpoints/credentials | Unknown/shared external resources recorded as unverified |
| Security | Operational separation for cooperating processes | Hostile-code containment requires a separately designed sandbox/VM |

**Strict verification profile:** refuse undeclared writable sharing known to the tool and unsupported required resources. Unknown application behavior remains **unverified**, not silently certified. The tool cannot detect every hardcoded URL, filesystem access or side effect in arbitrary source code. Do not advertise absolute isolation of native agents.

### DB policy

- Default to independent Postgres/Redis instances for destructive tests; permit shared Postgres with one DB and non-superuser role per environment when users deliberately choose it.
- Development and test targets must be separate. Workers, migration tasks and test commands receive the correct endpoint explicitly.
- Never use the user's primary database as a teardown target.
- Server extensions/version experiments require private instances, not separate schemas on a shared server.
- SQLite snapshots use backup API or verified quiescence, not a blind copy of live WAL state.
- Prepared Postgres templates are quiescent and owned by the tool; cloning must not terminate sessions against the primary dev DB.
- First MVP may use repeatable schema/seed recipes instead of cloning. State this honestly in output. Logical dump/restore comes before physical or filesystem-specific clone optimizations.
- Baselines include DB engine/version, seed/migration digest and creation method. Branch migrations run against the new private state, never the source baseline.

## 6. User/agent experience

All following CLI syntax is **proposed**, not implemented.

### Existing checkout: primary flow

```sh
# From a jj workspace, Git worktree, or ordinary checkout:
multidev plan --path . --profile review --json
multidev up --path . --name agent-auth --profile review --owner agent-17 \
  --wait --timeout 120s --json

multidev status agent-auth --json
multidev endpoints agent-auth --json
multidev logs agent-auth --service web --tail 100
multidev run agent-auth --task test --json
multidev verify agent-auth --suite review --json

# Preserve code and data when stopping:
multidev stop agent-auth --json

# Explicit destructive runtime/data cleanup; checkout is retained:
multidev destroy agent-auth --data --yes --json
```

`up` returns only after required resources, setup tasks, services and route probes succeed when `--wait` is requested. Failure returns typed diagnostics and retained logs. Human review environments stay alive until explicitly stopped; an agent's last task finishing is not an automatic teardown event.

### Optional checkout creation

```sh
multidev fork --from . --name agent-auth --vcs auto --json
# Returns checkout_path; then use the same up workflow.
```

- Default is a snapshot of the **specified source state**, including declared relevant working-copy changes. Starting from `main` instead is an explicit option, not a silent substitution.
- Detect jj first. In a colocated repo, use jj for mutations; never invoke mutating Git operations to emulate jj.
- Do not describe, rebase, switch, commit, or move bookmarks in the source checkout.
- Git tracked working changes require a defined snapshot transfer; Git worktree creation alone only gives committed contents. Report exclusions/conflicts, never pretend those changes were copied.
- Untracked source inclusion is explicit and bounded; credentials, runtime state and arbitrary ignored files are excluded by default.
- VCS adapters must use the installed CLI's supported commands, tested against an explicit version matrix. No guessed jj command sequence is a requirement of this draft.
- The initial product can omit `fork` and rely on Herdr/jj for checkout creation. Adoption is the mandatory primitive.

### Review and recovery

```sh
multidev list --json
multidev restart agent-auth --service web --json
multidev reset agent-auth --resource postgres --baseline fixtures-v1 --yes --json
multidev doctor agent-auth --json
multidev gc --dry-run --json
```

No command opens or focuses an application by default. Browser tools consume the returned URL in their authorized test session; native GUI automation needs separate permission.

## 7. Manifest and adapter design

Checked-in `multidev.yaml` defines the project contract; ignored/local overrides supply machine-specific settings and secret references. The schema is versioned, rejects unknown fields, validates references/DAGs, and distinguishes service/resource/task names.

**Illustrative schema sketch, not a finalized or executable config:**

```yaml
version: 1
project:
  id: shop
  name: Shop

resources:
  postgres:
    provider: postgres
    isolation: instance
    backend: native
    version: "16"
    databases: [development, test]
    baseline: fixtures-v1
  redis:
    provider: redis
    isolation: instance
    backend: container
    image: redis:7-alpine  # production schema supports digest pinning

services:
  web:
    runner: native
    command: [bundle, exec, rails, server, -b, "127.0.0.1", -p, "${services.web.ports.http}"]
    ports:
      http: {protocol: tcp, allocate: true}
    depends_on: [postgres, redis]
    setup: [prepare]
    env:
      DATABASE_URL: "${resources.postgres.databases.development.host_url}"
      REDIS_URL: "${resources.redis.host_url}"
    readiness:
      http: {port: http, path: /up, expected_status: 200, timeout: 60s}
    route:
      hostname: "web.${environment.label}.${project.id}.localhost"
      wildcard: true
  worker:
    runner: native
    command: [bundle, exec, sidekiq]
    depends_on: [postgres, redis]
    setup: [prepare]
    env:
      DATABASE_URL: "${resources.postgres.databases.development.host_url}"
      REDIS_URL: "${resources.redis.host_url}"
    readiness:
      exec: [./bin/worker-ready]

tasks:
  prepare:
    command: [bundle, exec, rails, db:prepare]
    needs: [postgres]
    env:
      DATABASE_URL: "${resources.postgres.databases.development.host_url}"
  test:
    command: [bundle, exec, rails, test]
    needs: [postgres, redis]
    env:
      RAILS_ENV: test
      DATABASE_URL: "${resources.postgres.databases.test.host_url}"
      REDIS_URL: "${resources.redis.host_url}"
    outputs: [tmp/test-results]

verification:
  review:
    tasks: [test]
    services: [web, worker]
    route_checks: [web]
```

The Rails commands here are only an example. A Go/Python/Node project uses the same schema and replaces executables, env bindings, resource choices and readiness probes. Rails-specific handling of hosts, tenant URL generation, test preparation, or tmp/PID paths belongs to an adapter or explicit project setup—not the core.

### Contracts

- **Runner:** plan/start/inspect/stop/exec/logs; native and container implementations.
- **Resource provider:** validate/plan/provision/inspect/reset/destroy, stable handles and capability declarations; optional consistent snapshot support.
- **VCS provider:** inspect/snapshot/create-checkout/rebind; jj, Git and no-VCS paths.
- **Router:** register/inspect/unregister owned routes; optional rb-portless adapter, no raw DB traffic.
- **Stack adapter:** suggest commands/env/probes/runtime paths from conventions. Output a reviewable manifest/diff; never silently edit application source or tracked configuration.
- **Reporter:** preserve arbitrary command exit codes/logs; add optional JUnit/structured format parsing without requiring stack knowledge.

Use argv execution by default; explicit shell commands require trusted manifest approval. Adapters/plugins run code and are not inherently safe. MVP uses built-in providers, no downloadable untrusted plugins.

## 8. Prioritized requirements

### P0: necessary for a useful safe pilot

| ID | Requirement | Acceptance |
|---|---|---|
| ENV-01 | Adopt checkout paths with jj/Git/no-VCS metadata | Branchless jj and detached Git boot; source VCS state unchanged |
| ENV-02 | Stable independent environment identity | Identical labels/revisions across projects never share resources |
| ENV-03 | Global allocation, per-env operations | Concurrent calls serialize correctly without blocking unrelated startup work |
| RUN-01 | Arbitrary native services, workers and tasks | Rails, Node and Go/Python fixtures use the same engine |
| RUN-02 | Durable supervised ownership | CLI exit does not kill services; foreign/PID-reused listeners never killed |
| RUN-03 | DAG/setup/readiness semantics | Dependencies wait for readiness; setup failure never publishes a healthy route |
| DATA-01 | Private data/resource lifecycle | Reset/migration in A leaves B and primary unchanged |
| DATA-02 | Private writable runtime paths | tmp/PIDs/logs/uploads/build output never resolve into another env |
| NET-01 | Ports and multi-plane endpoints | Host/container/browser receive usable distinct addresses |
| NET-02 | Owned local HTTP routes | Stopping A removes only A's routes, not shared proxy or B |
| CLI-01 | Noninteractive structured interface | JSON stdout, stderr logs, explicit errors/timeouts/targets |
| VERIFY-01 | Source-bound task evidence | Report fingerprints inputs and flags edits during verification |
| LIFE-01 | Idempotent stop/destroy/recovery | Interrupted operations reconcile; second teardown is a no-op |
| SAFE-01 | Default preserve-source and sharing policy | No implicit checkout deletion, host service changes or silent weaker mode |

### P1: after isolation gates pass

- Optional fork using jj/Git providers.
- Consistent baseline export/import, refresh and reset.
- Generic container/Compose integration and validated hybrid address resolution.
- MCP adapter wrapping the same CLI/control API; never a separate authorization path.
- Dependency-lock-aware CoW preparation, optional runtime/toolchain wrappers (mise/devenv/Devbox).
- Limits/idle shutdown with explicit policy; do not cool a pinned human review environment.
- Rich test parsers, diagnostic bundles and reusable stack adapters.

### Deferred

Dashboard/TUI, remote environments, automatic GitHub cleanup, physical DB snapshots, protocol-aware DB proxies, Nix-shell evaluation sharing, custom security sandbox, polyrepos and Windows. These are not MVP differentiation.

## 9. Lifecycle, concurrency, and ownership

### State machine

`planned → allocating → provisioning → preparing → starting → ready`

Failures enter `failed` with per-step status and owned handles. Ready may become `degraded`. Stop produces `stopped` while retaining data/code. Destroy passes through `destroying → destroyed`. Restart/reset increment a generation and invalidate prior readiness/results.

### Required semantics

- Each environment has a serialized operation lock; allocation registry transactions are short. Never hold a machine-wide lock across DB initialization, package installs or service startup.
- An operation journal records durable intent before external creation. Names/labels contain environment ID and operation token so interrupted creation can be reconciled even before the final handle was stored.
- Providers reconcile actual state vs desired state on retry; cleanup removes only validated owned handles. Failed startup retains bounded diagnostics by default; policy may roll back owned ephemeral resources, never source changes.
- `up` on the same environment/profile is idempotent. Config drift is visible; destructive reconciliation requires approval rather than silently resetting data.
- Setup tasks run once per relevant input fingerprint and successful resource generation, with explicit `always` mode. Non-idempotent migrations are not retried blindly after uncertain outcomes.
- Owner leases prevent accidental concurrent control. Agents can hand an environment to a reviewer with an explicit transfer/pin. Lease expiry alone never deletes data.
- Same-user lease tokens coordinate tools; they are not a security boundary against a malicious local process.

### Native process ownership

A small unprivileged supervisor owns native children and exposes a private Unix socket. Store launch nonce, birth/start identity, executable/control handle and process-group context—not only a PID or port. Signal only a validated owned process tree. If identity cannot be validated after recovery, report it and require manual adoption; do not guess.

Escaped/double-forked processes must either be tracked safely by the runner or marked unsupported. Linux process handles/cgroups and macOS supervision differ; prototype and test both before promising equivalent containment. Process-group termination alone is not proof of whole-tree ownership.

### Port semantics

Registry leases stop cooperating Multidev clients colliding, not arbitrary software. Ports are keyed by protocol/address; account for dual-stack wildcard overlaps, IPv6, multiple service ports and browser-blocked ports.

- Prefer socket activation when supported.
- Otherwise reserve/probe, hand off, then verify the child bound the expected endpoint. On a bind conflict, safely retry/re-render consumers within a bounded startup operation.
- Never identify the child solely by receiving a response from the port: a foreign process might answer it.
- Never kill whoever occupies a chosen port or silently route to them.
- Reallocation after readiness requires generation change and restart/reconfiguration of dependents.

## 10. Endpoints and routing

Expose separate endpoint fields:

- `browser_url`: named HTTP/HTTPS origin.
- `host_url`: direct loopback address for native consumers and health checks.
- `network_url`: private container-network service address.
- `socket_path`: when a resource supports it.

Runner/provider resolution selects the correct plane. Native → container uses loopback-published ports. Container → native requires a tested runtime-specific gateway, never an implicit assumption that container `localhost` means host. Reject unsupported combinations.

Services may expose several endpoints: web, HMR, inspector, gRPC, admin, metrics. Workers may expose none.

Default loopback-only; no LAN/public sharing. HTTPS requires explicit one-time trust setup. A nonprivileged proxy port is valid; if 443 is occupied, diagnose or use an approved existing backend rather than evicting it. Proxy lifetime is shared independently of any one environment.

Support tenant wildcard routes within each environment. Certificate issuance must match actual host depth. Probe browser-facing Host/TLS/app authorization separately from direct backend readiness, and include cookie-domain checks where configured.

## 11. Verification API and evidence

`run` executes a declared task in the environment's resolved context. `exec -- <argv>` supports ad hoc commands but reports them as ad hoc, not a declared suite pass. Concurrent destructive tasks within one environment need task/resource locks or separate test resource generations.

`verify`:

1. Ensures required resources/services/setup and route checks are ready.
2. Captures source/config/toolchain/baseline fingerprints.
3. Runs explicitly configured tasks and probes with deadlines.
4. Captures exit code, logs, optional parsed failures, output/artifact references.
5. Rechecks source/config fingerprints; changes invalidate the result as evidence for the new state.
6. Reports pass/fail/incomplete/stale/unverified coverage separately.

A green test task is not proof of full isolation or production correctness. Suites explicitly state what was exercised. Input identity includes VCS revision/change IDs where available **and content identity for relevant working/untracked files**; a bookmark or Git HEAD alone is insufficient. Missing reproducible toolchain identity is a caveat, not a fabricated fingerprint.

For live edits, before/after comparison detects common drift but is not transactional proof against transient edits. Strong evidence requires an immutable test snapshot/checkout or a cooperating edit freeze. Reports distinguish `live-checked` from `snapshot` assurance. Cached results never survive incompatible fingerprints.

Example redacted result:

```json
{
  "schema_version": 1,
  "environment_id": "env_01_example",
  "generation": 3,
  "status": "failed",
  "source": {"vcs": "jj", "content_digest": "sha256:example", "assurance": "live-checked", "changed_during_run": false},
  "inputs": {"manifest_digest": "sha256:example", "baseline_id": "fixtures-v1"},
  "checks": [{"task": "test", "exit_code": 1, "status": "failed", "log_artifact": "artifact:test-log"}],
  "isolation": {"database": "private-instance", "native_security_sandbox": false, "external_effects": "unverified"},
  "endpoints": {"web": {"browser_url": "https://web.agent-auth.shop.localhost"}}
}
```

Artifacts remain after stop/destroy under a configurable retention policy. Browser screenshots/traces can be attached by existing authorized browser tooling; the core need not invent a browser controller. Never claim browser verification unless that check ran.

## 12. Configuration, secrets, and trust

- Do not copy arbitrary source `.env` files automatically. Import allowed keys with explicit policy and keep per-environment overrides private.
- Secret references resolve at spawn, not into manifest/status output. Connection URLs returned to agents redact credentials; an explicit privileged execution path consumes full values without printing them.
- State dirs/files are private to the OS user. Log redaction is best-effort, not proof that arbitrary app logs are safe. Artifacts may contain sensitive data and require bounded local retention.
- Secret input identity uses provider/version identifiers, not raw values or public hashes of low-entropy secrets.
- Hook approval is tied to manifest/command identity. `--yes` acknowledges destructive runtime operations, not blanket approval of arbitrary untrusted checkout scripts.
- No automatic host package install, sudo, keychain trust, shell-profile edit or host-service stop.
- Imported Compose manifests reject/flag host networking, privileged mode, host PID namespace, Docker socket, unsafe bind mounts, fixed names/ports and unapproved external mutable volumes/networks. Approved exceptions weaken reported coverage.
- Existing source files are not rewritten by `up`. Required app changes are presented as a reviewable patch/setup requirement.

## 13. Proposed implementation approach

### Recommendation

Start with an Isola jj/identity and lifecycle feasibility spike. If extending it is viable, this PRD describes the extension requirements. If not, make a thin neutral control plane that delegates runners/routing rather than replacing every backend.

For a standalone implementation, **Go is a provisional choice**: portable binary, subprocess/network support, established local orchestration libraries and overlap with several prior-art projects. Rust is viable, especially for OS resource handling. Language choice should follow the adoption decision and ownership prototype, not lead it. A Ruby core would reuse rb-portless code but require Ruby installed in every target environment; retain Ruby as an optional routing backend instead.

### Components

```text
CLI / later MCP
        │
        ▼
local control API (private Unix socket)
        │
 planner ─ lifecycle journal ─ transactional registry
        │
 runners ─ resources ─ routes ─ VCS metadata ─ verification
```

- SQLite registry with schema migrations, transactions and unique active lease constraints; inspect-only JSON export.
- Per-environment dirs beneath user state, not primary checkout runtime dirs.
- Minimal supervisor for owned native processes; consider Process Compose as a runner after checking adoption/ownership/cleanup contracts.
- Native Postgres/Redis resource providers use installed executables with private data/socket dirs and endpoints; require correct versions and never install automatically.
- Optional container resource provider uses the installed OCI runtime; delegate Compose semantics through a validated generated override instead of rewriting Compose.
- Routing adapter can register endpoints with rb-portless. Prototype a **non-takeover, owned-route registration contract** before integration; do not assume its existing CLI provides the required guarantees.
- Project commands/toolchains are invoked from declared wrappers; do not reimplement Nix/mise/Devbox.

### Build sequence and release gates

**Phase 0 — adoption experiment.** Use the research test matrix. Deliver pass/fail evidence, setup effort and contribution vs build decision. No standalone implementation until this gate closes.

**Phase 1 — native operational MVP.** Adopt paths; neutral manifests; supervised app/worker/tasks; port leases; private Postgres/Redis via installed binaries; separate dev/test targets; HTTP routing backend; JSON status/logs; idempotent stop/destroy; basic verify evidence. Baseline schema/seed recipes are sufficient. No promise of generic cloning yet.

**Phase 2 — baseline and hybrid support.** Consistent dump/restore/SQLite backup, optional fork, validated container resources/Compose runner, TLS/routing integration, working output/dependency isolation. Promote only after platform parity tests.

**Phase 3 — agent ergonomics.** MCP/Herdr adapters, richer artifacts/parsers, limits/GC/pinning, configuration suggestion adapters. These must call the same lifecycle engine, not bypass ownership rules.

No fixed delivery estimate until the OS ownership prototype, prior-art trial and representative app setup are measured.

## 14. Acceptance tests and success metrics

### Release-blocking scenarios

1. Three simultaneous environments on macOS and Linux; native Rails, Node and Go/Python fixtures share the same engine.
2. Bookmarkless jj, detached Git and plain-directory adoption; no source history/branch/bookmark mutation.
3. Concurrent duplicate `up` requests; identical labels in separate projects; restart after interrupted allocation.
4. Migrate/reset development and test DBs in A; B and primary retain data and uptime.
5. Private Redis `FLUSHALL` in A leaves B untouched; shared Redis modes are labeled weaker.
6. HTTP, tenant Host routing, cookies, HMR/WebSocket, DB connections and worker behavior checked—not only listening ports.
7. Foreign listener occupies desired port; stale/recycled PID stored; cleanup never signals foreign processes.
8. Crash during provisioning/setup/start/destroy; retry reconciles only owned resources. Destroy twice; journal corruption produces safe failure.
9. Env file or output path attempts to escape; no tracked config/env/source rewritten without explicit action.
10. Edit source during verification; result becomes stale/incomplete. Snapshot verification remains bound to its immutable state.
11. Teardown retains checkout and verification artifacts; cannot stop the shared proxy or delete primary DB.
12. No hidden GUI activation, clipboard changes, audible tests or unauthorized trust/system-service modifications.

### Initial measurable targets (hypotheses, not benchmarks)

- Zero cross-environment data/process/source mutations across the acceptance suite.
- Zero leaked owned resources after completed teardown; incomplete cleanup is explicitly reported and retryable.
- Correct per-env endpoints and input fingerprints for every successful verification.
- Ten concurrent allocation/start attempts without duplicate leases; three full app environments as the initial laptop capacity scenario.
- Warm lightweight fixture startup under 10 seconds excluding app-specific setup; realistic cold/warm times recorded separately, no universal promise.
- One project manifest serves multiple agents without manual per-worktree port/data edits.
- Compare setup effort, boot reliability and resource cost against adopt-first candidates, not only against hand-written scripts.

## 15. Open decisions

1. Can Isola accommodate jj path identity, stronger lifecycle ownership and private server resources cleanly?
2. Is Docker/Nix acceptable as an optional dependency—or is a no-Docker native workflow mandatory?
3. Do typical tests need private DB instances, or can constrained shared Postgres roles satisfy most cases?
4. Which app needs the first real pilot? This workspace is empty; no current Rails service config was available to inspect.
5. What baseline size/refresh/retention needs matter, and can sanitized seed data replace live-dev clones?
6. Does Herdr expose reliable create/adopt/teardown hooks for the user's jj workflow? Do not assume a particular API.
7. Can rb-portless expose owned route registration/removal without process takeovers or proxy lifecycle coupling?
8. Which mutable output paths/toolchains require adapters before independent execution can be promised?
9. Is a thin extension/wrapper meaningfully cheaper than maintaining another daemon and resource-provider ecosystem?

Resolve these through the Phase 0 experiment. The broad project category exists; the remaining opportunity is a better fit for this workflow, not the invention of parallel environments.
