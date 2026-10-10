# Workspace organization and local development environments

Version: **0.4, Rust architecture selected**  
Date: **2026-10-10**  
Product name: **Parcela**, selected by the user.  
Status: **implementation authorized and in progress; runtime acceptance tracked in [implementation status](implementation.md)**.

This replaces the earlier adoption-first PRD. [Version 0.1](PRD-0.1.md) is historical, not the active specification. The existing Ruby pilot is an experiment and does not implement this specification.

## 1. Product decision

Build a **separate, service-agnostic workspace and environment organizer**. Combine the useful portless experience with native jj/Git checkout awareness, safe environment ownership, and Process Compose organization. Routing is one capability, not the whole product.

Keep rb-portless as a Ruby port of Vercel's portless. This project has no upstream feature-parity obligation and must not expand rb-portless into a different product.

> Organize your workspaces and run each with its own services, state, and identifiable URLs, without editing configuration per checkout or disturbing another environment.

A new project is justified by the desired integrated workflow, not by inventing a new reverse proxy or process scheduler. Prefer maintained implementations for TLS/HTTP and supervision. Our code owns the workspace, naming, endpoint, state, and lifecycle integration.

### Confirmed requirements

- Browser-facing URLs have no explicit port: `https://shirabe.localhost`, not `https://shirabe.localhost:8443`.
- The designated primary checkout can use `shirabe.localhost`; previews have distinct, identifiable URLs.
- Support the main portless workflow, including running one arbitrary command without Process Compose.
- Support several simultaneous jj workspaces and Git worktrees, including bookmarkless/detached checkouts.
- One maintained project configuration works across checkouts. Runtime records may differ; users do not maintain a configuration per worktree.
- Provide our own CLI to wrap and organize Process Compose. Delegate scheduling, recovery, and its service TUI instead of implementing another supervisor.
- Default to flat environment hostnames and tenant wildcard routing off; allow explicit configuration of either.
- Project-defined commands decide preparation, seed/snapshot policy, migration, testing, and reset. Core manages context and ownership, not a hardcoded database workflow.
- First POC: Shirabe, Rails + SQLite, with web/CSS/jobs organized through Process Compose.
- Include optional private Tailscale and per-environment LAN sharing. ngrok and public Funnel are outside current scope.
- Remain service/framework agnostic. Rails, Node, Go, and Python integrations must be optional conventions, not core requirements.
- Reuse the project's dependency tooling. Do not require Nix, Docker, a Ruby Gemfile, or a Node package manifest merely to use the router.
- Support an always-on host and private phone access when Tailscale sharing is enabled.
- Trim features that do not support these jobs. No compatibility promise for every portless flag or internal mechanism.

### Selected implementation architecture

- Rust owns the CLI, workspace registry, configuration, environment lifecycle,
  task execution, endpoint ownership and integration.
- Caddy is the managed external HTTP/TLS engine; Process Compose is the managed
  external service supervisor. No embedded backend or retained Go scaffold.
- Project configuration owns commands and data policy. Switching languages does
  not replace either maintained backend or change the safety/isolation contract.
- The Rust foundation's executable schema/commands are documented in
  [configuration](configuration.md); coverage and gaps are recorded in
  [implementation status](implementation.md). Adoption/context do not start
  services, and planned URLs are not runtime acceptance.

### Proposed operational defaults

- macOS and Linux first; no Windows promise for the initial release.
- HTTPS by default, loopback-only, standard ports 443 and 80; explicit one-time setup/trust.
- One product installation supplies/manages its routing engine without requiring users to maintain a Caddyfile. How that is packaged is a technical spike, not a user-facing setup choice. Process Compose availability/version is checked explicitly.
- Data policy comes from each project; no universal fresh-database or clone-development default. A generic consistent-snapshot helper can be added without becoming a provider ecosystem.
- mDNS name publication is an optional convenience for LAN discovery, not required for LAN sharing.

## 2. Users and workflows

1. A developer runs their primary app at the same URL every day.
2. An agent adopts a checkout created by jj, Git, or Herdr and obtains its own runtime and URL.
3. Several agents run independent migrations, tests, workers, and asset servers without cross-environment changes.
4. A reviewer opens a preview without stopping the primary environment.
5. A user optionally opens that preview from a phone through their tailnet.

The product does not manage agents, merge changes, rebase/commit on behalf of users, or deploy production applications. Existing checkouts can always be adopted. Whether optional workspace creation/removal belongs in v1 remains F1 below; those operations would delegate to jj/Git, not replace them.

### Workspace organization

Required organization features:

- Register/discover projects and their checkout inventory; distinguish primary and preview roles.
- Show path, workspace label, VCS metadata where available, source identity, running state, services, URLs, private paths, sharing modes, and last task result.
- Support current-checkout commands and explicit workspace targets; provide project-scoped lists and an explicit machine-wide view.
- Start, stop, restart, inspect/log, and run project tasks in one environment or explicitly selected environments. Bulk operations show targets first; destructive all-project operations are never an implicit default.
- Attach to the appropriate Process Compose service TUI when requested; do not create a separate dashboard or activate native applications.
- Group named configurations/profiles such as normal development and safe review, without manually copying files between checkouts.
- Report missing, moved, stale, unhealthy, or externally managed checkouts without deleting data or taking over processes.

Workspace creation, source deletion, switching another terminal/editor, and VCS history manipulation are separate from ordinary environment lifecycle.

### Project-defined tasks and data policy

A project can define setup/prepare, migrate, seed, test, snapshot/restore, and reset commands, with required context, dependencies, explicit outputs, and timeouts. Commands are optional and arbitrary executables, not framework-specific magic. Finite orchestration uses Process Compose tasks where appropriate; the wrapper resolves targets and preserves results.

Initialization and repeatable preparation are distinct: installing/restoring a large baseline should not automatically rerun on every `up`. Record successful input fingerprints/generation, permit explicit refresh, and never retry an uncertain destructive command blindly. Commands may use Bundler, Rails, npm, SQLite, Docker, or another existing tool without the core becoming its dependency manager.

## 3. Name and positioning

[Name candidates](names.md) contains **100 current options** focused on English, Spanish, and two-word compounds. The user's style benchmark is Devbox: compact, descriptive, and suggestive of a complete environment. The [earlier 120-name brainstorm](names-0.1.md) is historical, not the current shortlist.

Positioning: **organized workspaces with runnable local environments**, not merely a router, port mapper, package manager, or branch manager. The name should still make sense for a plain directory, jj workspace, or an application without Git branches.

Selection criteria:

- Short enough to type as a command; easy to pronounce and spell.
- ASCII command spelling, with original spelling allowed in branding.
- Suggest a place, passage, connection, or independent working area.
- Do not force Ruby/Node, Git branches, or a particular supervisor into the identity.
- Before selection: check CLI/package/repository conflicts, trademark concerns, native-language connotations, and desired domain availability.

Availability has **not** been checked. A name's appearance here is not a uniqueness or linguistic endorsement.

## 4. Scope and release boundaries

| Capability | Proposed priority | Boundary |
|---|---|---|
| Standalone arbitrary-command routing | P0 | Portless-style `run`, no orchestration required |
| Standard-port named HTTPS | P0 | No silent high-port fallback |
| jj/Git/plain-directory adoption and workspace inventory | P0 | Stable identities, project grouping, metadata without source-history mutation |
| Primary and preview URLs | P0 | Persisted ownership, not branch-as-identity |
| HTTP/1.1, HTTP/2, WebSockets, SSE/HMR | P0 | Through a maintained HTTP/TLS engine |
| Explicit aliases and optional tenant subdomains | P0 | Tenants off by default; guard namespace overlap when enabled |
| Process Compose organization through our CLI | P0 | One graph, separate instance/control endpoint per environment; tasks/logs/restarts/optional TUI |
| Private runtime and declared resource bindings | P0 | Routing-only mode must not imply DB isolation |
| Status, endpoints, logs, diagnostics, JSON | P0 | No dashboard required |
| One-time trust and optional boot service | P0 | Explicit authorized setup, never unattended prompts |
| Main-to-preview cutover/migration diagnostics | P0 | No automatic takeover of occupied listeners |
| Declarative multi-service applications | P0 | No package-manager workspace-discovery maze |
| Project-defined setup/data/test/reset commands | P0 | Policy per project; context/ownership/result tracking in core |
| Shirabe Rails + SQLite POC | P0 | Exercise the complete workspace/services/URLs/state flow, not just routing |
| Optional private Tailscale hostname per environment | P0 feature goal | No shared-host/different-port shortcut; prerequisites and separate-device verification required |
| Optional per-environment LAN sharing | P0 feature goal | Route-level opt-in, standard ports, no exposure of unshared projects |
| Optional mDNS LAN naming | P0 feature goal | Name resolution only; gracefully diagnose unavailable/multicast-blocked discovery |
| Generic SQLite snapshot helper | P1 | Project chooses policy; never copy live WAL files blindly |
| Built-in Postgres/Redis providers | Not required for v1 | Projects may declare installed services/containers and safe recipes |
| Custom owned private DNS domain | P1 | User supplies DNS/TLS prerequisites |
| Generic containers/hybrid address resolution | P1 | Explicit supported network planes |
| ngrok and public Funnel | Excluded from current scope | No public-tunnel implementation required |
| MCP, GUI/TUI, plugin marketplace, idle scheduling | Deferred | Existing CLI/JSON and Process Compose cover initial needs |

P0 means required for the proposed first integrated release, not implemented today. Questions may change release allocation. The isolation contract is never optional simply because a provider is deferred: unsupported resources are reported as unsupported.

## 5. Identity and naming

### Stable identities

- **Project ID:** an explicit identifier, separate from its display name.
- **Checkout:** canonical source directory plus VCS metadata where available.
- **Environment ID:** immutable generated identity, independent of directory basename, branch/bookmark, revision, PID, or display label.
- **Role:** primary or preview. A role does not change when a bookmark moves or the user edits the working revision.
- **Label:** human-readable name used in URLs; unique within the project's namespace.
- **Generation:** changes when endpoints/configuration are reallocated or reset.

Detect jj before Git in colocated repositories. Never run mutating Git commands to emulate a jj workspace. Read the current checkout's metadata, not an unrelated checkout's active branch. Detached/bookmarkless checkouts are normal.

Infer labels from workspace/worktree metadata where reliable; allow an explicit label. Sanitize deterministically, enforce DNS limits, and report collisions rather than silently take another environment's hostname.

Identify the primary through VCS checkout metadata or explicit adoption and persist the role. Never promote the first preview started merely because no primary is running. If ambiguous, require explicit primary designation; noninteractive commands fail with a useful remedy.

Checkout moves require an explicit rebind after identity validation. A missing source path is not permission to delete its runtime or data. Two environments sharing one source directory are not independent code-editing environments.

### Default URL scheme (flat environment bases)

| Target | URL |
|---|---|
| Primary default service | `https://shirabe.localhost` |
| Auth preview default service | `https://auth-shirabe.localhost` |
| Billing preview default service | `https://billing-shirabe.localhost` |
| Explicit auth preview API service | `https://api.auth-shirabe.localhost` |
| Auth preview tenant, only when enabled | `https://kobe.auth-shirabe.localhost` |
| Configurable nested alternative | `https://auth.shirabe.localhost` |

The primary default service has no `main.` prefix. Flat environment bases avoid placing previews under the primary cookie domain. Tenant/wildcard routing is off by default; additional services are explicitly registered and do not require tenant mode. Additional services have explicit names. Service, environment, and tenant labels occupy overlapping DNS namespaces; configuration must reserve conflicting names or require explicit disambiguation.

Changing a label or canonical URL is explicit and updates consumers together. Arbitrary aliases do not change environment identity. Alias ownership is retained while stopped unless explicitly released.

## 6. Portless functionality retained

### Run an arbitrary command

A standalone run accepts literal argv after `--`, selects a backend endpoint, injects resolved environment values, starts the child, waits for configured readiness, and registers its route. Forward termination signals and propagate exit status. No automatic application restart policy in this mode; a multi-service restart policy belongs to Process Compose.

Baseline environment contract: `PORT`, `HOST=127.0.0.1`, canonical browser URL, project/environment identity, and private runtime directory. Final variable names are undecided. Unix-socket-capable applications can opt out of TCP port allocation.

A generic command must consume its declared endpoint. The tool cannot make an arbitrary hardcoded listener configurable. Fixed backend ports are permitted only when available and ownership-safe; a collision must not cause another listener to be killed.

### Naming without configuration

Allow an explicit project/name with a command. Otherwise infer a project display name from the checkout directory/repository and report it. For an ordinary directory, a standalone primary app is supported. Zero-config routing does not mean zero-config data isolation.

### Framework assistance

Small, opt-in adapters may supply known server flags, host authorization, canonical URL generation, and WebSocket origins. Never inject server flags into a build/test command, guess through arbitrary shell pipelines, or silently rewrite source. Rails must not require the new proxy implementation in its Gemfile. Framework configuration remains project-owned and reviewable.

### Aliases and externally started applications

Register an existing HTTP backend without taking over its process. Record whether the route is managed or externally owned. Removal deletes only the route; it never stops an external process. An HTTP alias to port 5432 is not a PostgreSQL proxy. Arbitrary raw TCP name-based routing is outside the initial product.

### Lookup and diagnostics

Print the current service URL for scripts, list active/stopped environments, inspect route owners and effective configuration, and diagnose listener conflicts, TLS trust, name resolution, unhealthy backends, and stale registration.

Stale-route cleanup is ownership-validated and dry-runnable. Do not copy portless's force-takeover or kill-by-port behavior. Global removal of trust/state is a separate explicit administrative operation, not ordinary environment teardown.

### Multi-service and monorepo use

Support explicitly declared web/API/worker/asset services using any language. A worker may have no endpoint. Standalone mode wraps one command; multiple long-running services use Process Compose or an existing supervisor. Automatic npm/pnpm/yarn/bun workspace discovery and turborepo-specific orchestration are not required.

## 7. Router, HTTPS, and listener ownership

One machine-level front door serves all projects and environments. App/backend ports may be dynamic and internal; browser origins remain on standard HTTPS 443. HTTP 80 redirects only to the matching registered HTTPS origin.

- Bind loopback IPv4 and IPv6 by default. No implicit LAN/public listener.
- Never silently substitute `:8443`, `:1355`, or another browser-facing port when 443 is occupied.
- Diagnose the incumbent and require explicit operator migration/adoption. Do not kill, stop, or reconfigure it automatically.
- Arbitrary ports are allowed only in explicitly selected test/debug profiles. These are not the normal product experience.
- Install privilege/trust/boot-service support through a separate one-time action. App commands and Process Compose run unprivileged.
- Keep the shared router alive independently of any environment. Stopping one preview removes only its owned routes.
- Preserve request host, scheme, port, WebSocket upgrades, streaming, and forwarded headers correctly. Only trust headers from configured proxy boundaries.
- Match certificates to actual hostname depth. Do not assume a single `*.localhost` certificate covers every host.
- Bound request/header waits; distinguish unknown/stopped routes, unhealthy backends, and router failure.
- Reject proxy loops and route targets back to the shared listener.
- Unknown external clients must not receive a listing of local projects.

### Exact routing before wildcard routing

Use exact and most-specific matches, but also reserve stopped environment namespaces. If `auth.shirabe.localhost` stops, the primary `*.shirabe.localhost` tenant route must **not** serve that hostname or descendants of the auth namespace. Return an unavailable/not-found response instead. Persist reserved names across restart and handle conflicts deliberately.

Route registration includes environment ID, generation, and an ownership token. A stale cleanup operation cannot remove a successor's route. Route/config updates are serialized or use compare-and-swap semantics, with recoverable intent recorded before external changes.

### Coexistence with rb-portless/Caddy

Both binaries can exist on the machine. They cannot independently own the same address/port, even for different hostnames. IPv6 and both HTTP/HTTPS listeners count.

The default migration proposal is an explicit front-door cutover, leaving rb-portless's code and other project configuration untouched until migrated. If retaining existing rb-portless projects is required, design a dedicated coexistence/import mode; do not promise transparent chaining. rb-portless currently overwrites forwarded scheme based on its own listener mode, so an HTTPS-to-HTTP proxy chain requires a separately verified scheme/CSRF contract.

No existing Caddy instance may have its complete configuration replaced unless the operator explicitly dedicates it to this product. If a managed Caddy backend is used, prefer a private, permissioned Unix-socket admin endpoint; never expose an unauthenticated config API to the LAN.

## 8. Browser isolation and application integration

Distinct ports do not isolate cookies, local storage, or browser origins. Default flat bases (`shirabe.localhost`, `auth-shirabe.localhost`) avoid nesting a preview below the primary. Host-only cookies remain preferable. Tenant mode is off by default; enabling it requires an explicit environment-scoped cookie/origin policy. Flat names do not protect applications that deliberately set a broader shared cookie domain.

For nested names, the primary `shirabe.localhost` application must use **host-only authentication cookies** if previews live below it. A primary cookie scoped to `shirabe.localhost` would also be sent to `auth.shirabe.localhost`. Scoping preview cookies alone is insufficient.

Tenant login sharing must be designed explicitly. A broad parent cookie cannot both cover all primary tenants and exclude nested previews. Options include flat environment names, separate tenant namespaces, or request-level login/session handling. The router cannot transparently fix this by rewriting every cookie.

Each stack integration must cover host authorization, canonical URLs in workers/mailers, secure-cookie behavior, CSRF scheme/origin checks, WebSocket origins, OAuth callback configuration, and environment-specific secrets where required. Do not broadly disable these safeguards.

Database separation does not automatically prevent email, payments, uploads, analytics, or webhooks touching shared external systems. Use declared safe profiles/credentials, record unknown effects, and never certify native processes as sandboxed.

## 9. Process Compose and configuration model

There is **one maintained project specification**, inherited by every checkout. Environment-specific generated values are machine/runtime state, not additional user-maintained configuration files.

Our CLI is the normal entry point and wraps Process Compose. Proposed layout: one project YAML file containing organization/routing/task metadata and a pass-through native Process Compose graph. F2 asks whether to choose that or preserve a separate existing Process Compose file. Do not assume arbitrary new fields are accepted by Process Compose's parser.

Requirements regardless of layout:

- The project defines commands, graph, readiness, and restart policy once.
- Our layer resolves identities, endpoints, private paths, and route ownership.
- Process Compose owns service dependencies, startup/recovery/restarts, logs, TUI/API, and shutdown.
- Each environment has its own Process Compose instance, private control endpoint, and supervisor log. Namespaces inside one shared instance are not isolation.
- Use a private Unix control socket with explicit Unix-socket mode; setting only a socket pathname is insufficient.
- All processes/tasks receive the same resolved generation of values. Workers and tests must not retain another environment's URLs/data bindings.
- Route visibility distinguishes starting from ready. A startup failure cannot be advertised as healthy. Restarts return routes to a starting/unavailable state before serving a new healthy backend.
- `<tool> up` is the normal integrated startup. It delegates the Process Compose graph without inventing a second scheduler or silently installing dependencies.
- Direct `process-compose up` is an advanced compatibility option, not the required default interface. It needs maintained hooks/resolved context if supported; it is not automatically workspace-aware.
- The environment layer must retain route ownership across service restarts and reconcile after interrupted shutdown. A cleanup shell task alone is not proof of crash safety.

Proposed manifest concepts: project ID/name, default service, route layout/domain, service endpoint mappings, private paths, environment bindings, initialization/reset tasks, and an unchanged Process Compose graph. The initial Rust foundation schema is documented in [configuration](configuration.md); lifecycle/resource bindings and complete runtime commands are not finalized. Organizer validation does not validate backend graph compatibility.

## 10. Ports, sockets, runtime, and data

### Endpoint allocation

Prefer private Unix sockets when an application supports them. Otherwise use durable, machine-wide leases keyed by address/protocol, account for IPv4/IPv6 wildcard overlap and browser-blocked ports, and verify the actual child listener. Probe-and-release is not an atomic bind handoff. Use socket activation where supported; otherwise retry only owned allocation with bounded reconfiguration.

Never infer ownership solely from a port response or PID. Validate the control handle, launch nonce/birth identity, and expected process before signaling. If validation is unavailable after recovery, report an orphan requiring explicit adoption rather than guessing.

### Isolation levels

| Mode | Promise |
|---|---|
| Routing-only | Own hostname and route; data/process sharing remains explicit and unverified |
| Managed environment | Declared runtime/data/endpoints are private and lifecycle-owned |
| Shared resource | Visible opt-in with narrower guarantees; not labeled fully isolated |

Separate checkouts provide independent code edits. Known writable outputs must also be private: SQLite, uploads, queue/cache, tmp/PIDs, test scratch, logs, generated assets, and caches. Two runtimes using one checkout require special handling and are not an initial independent-editing guarantee.

### Data lifecycle

- Development and test have separate targets; migration/reset never points at the primary environment's database.
- Primary user-owned data may be adopted for development but is never an automatic deletion/reset target.
- Each project selects fresh schemas, seed commands, a consistent baseline, or another declared initialization recipe. Core does not decide whether a project needs a copied development DB. Environments without required dictionary/user data must say so.
- A snapshot is consistent and immutable before cloning. SQLite uses backup API or verified quiescence, not a blind copy of live main/WAL files.
- Queue, cache, and scratch start fresh by default. Copying pending jobs is explicit and requires a safe external-effects policy; a preview must not replay another environment's emails, uploads, or payment work.
- CoW cloning is an optimization of a verified baseline, not permission to clone a live source. Record baseline engine/schema/content identity.
- Postgres isolation can use a private instance, or an explicitly weaker constrained shared server. Redis prefixes/logical DBs do not protect against FLUSHALL or server-wide changes.
- Declared recipes can invoke existing binaries/Docker/Compose. Core does not implement database engines, generic package installation, or arbitrary production-data copying.
- Reset/destroy require explicit scope and confirmation in human mode, or explicit noninteractive consent. Validate ownership and canonical paths; never sweep arbitrary source/home paths.
- Stopping preserves code/data. Removing a workspace externally does not authorize deleting its data.

Project-defined commands are the confirmed data mechanism. Database-provider policy is not hardcoded into the initial core. The Shirabe POC chooses a safe recipe in its own configuration; other projects choose theirs. Unsupported required resources fail or are marked unverified, never silently shared.

## 11. Lifecycle and control surface

Target public actions; only the foundation subset in [configuration](configuration.md) exists today:

| Action | Contract |
|---|---|
| `run -- COMMAND...` | Standalone command, named route, resolved endpoint, exit/signal propagation |
| `up` | Start current environment through Process Compose; optional wait for readiness |
| `workspace list/adopt/inspect`, `status`, `url`, `logs` | Organize projects/checkouts and inspect current or explicit target without changing source/history |
| `restart`, `task NAME -- ARGS...`, `attach` | Delegate service restart/task execution/service TUI to the correct Process Compose instance |
| `context` | Idempotently resolve context for native Process Compose integration |
| `route register/inspect/remove` | Explicit HTTP endpoint ownership; never force-kill incumbents |
| `stop` | Stop only owned environment services/routes; retain code/data |
| `reset`, `destroy --data` | Explicit resource operations, only when the provider supports them |
| `doctor`, `gc --dry-run` | Diagnose and preview ownership-validated cleanup |
| `proxy status`, `setup`, `trust`, `service ...` | Explicit shared-router installation/administration |
| `share --tailscale`, `share --lan`, `unshare` | Optional owned exposure of this environment; mDNS LAN naming may be enabled separately |

Default target is the current checkout; explicit paths/IDs support agent operations. Destructive ambiguous targets fail. No hidden browser opening, clipboard changes, focus changes, or audio.

JSON output contains stable IDs, resolved URLs, health, generation, owned handles, capabilities, and caveats. Logs go to stderr/files, not mixed with JSON. Preserve command exit codes; return distinct setup/conflict/timeout/unsupported failures. Noninteractive operation never waits for sudo, trust, or a web login.

State transitions: resolved → allocating → preparing → starting → ready; failure retains diagnostics and owned intent. Stop → stopped; explicit reset increments generation; destroy removes only validated managed resources. Repeated operations are idempotent, and interrupted operations reconcile from recorded ownership.

Use a permissioned control API and a transactional durable registry. Same-user native code can still bypass operational boundaries; this is not hostile-code containment.

## 12. Optional sharing and always-on operation

`.localhost` resolves on the client and therefore does not reach the host from a phone. Remote URL fields are separate from local URLs.

Optional remote provider, included in the desired feature set: Tailscale Services with a hostname per environment, for example `https://auth-shirabe.<tailnet>.ts.net`. Tailscale HTTPS uses standard 443 on its service address; users do not select preview ports. Do not assume arbitrary subdomains of a node's MagicDNS name work automatically.

Provider requirements:

- Explicit opt-in per environment, private tailnet only by default.
- Service definition/access policy, tagged compatible host, approval/auto-approval, HTTPS prerequisites, and client support checked before success.
- Persist service and registration ownership. Compare actual config before changing/removing it; no global `serve reset`, no port-based guessing.
- Handle pending approval, unavailable hosts, interrupted registration, and safe unshare.
- Keep application backends loopback/private; do not open the whole shared local router to the network.
- Update canonical URLs, allowed hosts, mailers/workers, cookie scopes, WebSocket origins, and callback rules together.
- Validate from a separate authorized tailnet device before marking remote access verified. A local HTTPS probe is insufficient evidence of phone access.
- State that TLS names may appear in certificate-transparency logs even though access is private.

### LAN sharing and mDNS

LAN mode exposes only explicitly opted-in environments on the local network. Opening a shared listener must not expose every registered project; enforce the sharing flag per route and fail closed on unknown peer identity. Do not trust a client-supplied forwarded address as proof of loopback origin. Unsharing A preserves B's exposure and the shared router.

Optional mDNS (multicast DNS, often called Bonjour discovery) publishes a concrete `.local` hostname such as `shirabe.local` or `auth-shirabe.local`, allowing compatible devices on the same local network to resolve it without a DNS server or `/etc/hosts` edits. It is not Tailscale MagicDNS, does not usually cross subnets/VPNs, may be blocked by Wi-Fi isolation, and does not supply arbitrary wildcard tenant records.

Discovery is separate from HTTPS trust. A phone still needs to trust the local CA for LAN HTTPS; publication does not make a certificate trusted. An explicit HTTP-only LAN profile can be permitted with a security warning. Diagnose mDNS failures and offer a clearly marked LAN-IP/Host-header test fallback, not a promise that normal browser/IP HTTPS has the correct name/certificate. Tailscale is the preferred phone path when installing a local CA is undesirable.

An always-on host runs services persistently through explicit boot configuration. Human review environments remain running until stopped unless an explicit retention policy exists. No automatic idle suspension in v1. Custom domains require owned DNS and a separate TLS strategy; no implicit wildcard MagicDNS or local-CA trust on the phone.

## 13. Architecture and maintenance limits

```text
CLI / Process Compose integration hooks
                  |
      workspace + environment control
      identity / registry / endpoint leases
      ownership / private paths / route policy
          |                       |
 Process Compose             HTTP/TLS engine
 app / assets / workers      one shared front door :443
          |                       |
 existing tools/data         optional private Tailscale exposure
```

Selected architecture: Rust control plane, managed Caddy for HTTP/TLS, and Process Compose for service supervision. Do not embed a Go implementation or retain the abandoned Go scaffold. The user experience is one product install/setup, not maintaining a separate Caddyfile or launching both CLIs. Process Compose remains the maintained supervisor; setup reports its prerequisite/version explicitly and does not install software during an ordinary run.

For managed Caddy, disable automatic trust installation during ordinary runtime; perform trust explicitly. Isolate its state/config/admin endpoint from any user-managed Caddy instance. Serialize owned changes or use ETag/If-Match concurrency control; do not assume multiple admin requests form a transaction.

Keep the initial provider set small. No interchangeable-supervisor framework, plugin marketplace, giant framework detector, generic agent integration, telemetry requirement, or new TUI. Reuse Process Compose's mature UI/control features. HTTP/2-to-backend, custom user cert/key plumbing, Windows GUI trust, ngrok/Funnel, and npm monorepo discovery are not initial differentiators.

Code reuse must preserve licenses/notices. rb-portless is MIT; upstream portless is Apache-2.0. Copying an idea is not evidence that implementation code is compatible with the chosen distribution license. Select the project license before importing code.

## 14. Shirabe reference integration and existing evidence

Inspected project: `/Users/davafons/workspace/shirabe`. Existing proxy source: `/Users/davafons/workspace/rb-portless`. Neither is literally a sibling of this directory.

Shirabe uses Rails, Tailwind, out-of-process Solid Queue, Solid Cache, and SQLite. Development has primary/dictionary/queue/cache databases; tests use separate primary/dictionary databases and slot-scoped scratch. Development Cable uses the async adapter. PostgreSQL/Redis examples from the older draft do not describe this application.

Required Shirabe setup:

- Primary URL `https://shirabe.localhost`; default preview URL `https://auth-shirabe.localhost`, tenants disabled. Nested and tenant layouts remain configurable.
- Independent development SQLite files, uploads, tmp/PIDs, logs, assets, and existing `bin/test` slot handling per checkout.
- Web, CSS watcher, and jobs under Process Compose; prepare schema/assets before dependent services.
- Correct host authorization and URL generation without requiring rb-portless's proxy library in the application bundle.
- An explicit auth-cookie change if nested previews are used. Current `domain: :all` can span environments, including primary-to-preview leakage.
- Safe preview external-effect settings; do not open letter_opener/browser windows or reuse production upload/payment credentials during agent tests.
- The dictionary is large (about 16 GiB at inspection). Shirabe's project recipe must choose a consistent baseline/import, a small fixture corpus, or explicit fresh initialization and report its storage/readiness limits. A fresh schema is not a full searchable reference dataset. This is POC configuration, not a product-wide data-policy choice.

Observed pilot evidence, not release acceptance:

- Two disposable tracked-source copies were prepared, about 57 MB each; they excluded existing databases, secret env files, browser caches, and dependency trees.
- The pilot changes auth-cookie scope and mail delivery only inside those copies, not the user's source checkout.
- Ruby syntax checks, a dependency check in one copy, and Process Compose v1.122.0 config dry-run passed.
- Process Compose was downloaded to a temporary tools directory and checked against the release checksum, not installed globally.
- The initial broad copy hit disk exhaustion on an ignored extension/browser cache. Only the failed agent-owned copy was removed; copying was changed to bounded tracked files.
- Services were **not booted**. TLS, actual auth, routing isolation, data isolation, teardown, and phone access remain unverified.
- The old `bin/multidev` pilot uses rb-portless and generated per-copy YAML. It is not the architecture or CLI compatibility baseline for this new project.

## 15. Release-blocking acceptance scenarios

1. One generic command works without Process Compose, using a named standard-port HTTPS URL, with exit codes/signals preserved.
2. A designated primary and two previews run together at the expected URLs; bookmark/revision edits do not change identity.
3. Branchless jj, detached Git, and a plain-directory fixture work without source/history mutations. Test jj and Git independently.
4. One maintained configuration resolves across all checkouts. No manual per-workspace ports/paths/config copies.
5. Rails and Node plus a Go/Python fixture exercise the same neutral API. Unsupported command port behavior is diagnosed rather than guessed.
6. Real browser-facing Host/TLS, HTTP/2, WebSockets/HMR, SSE, redirects, CSRF, and URL generation work. Unknown hostnames cannot mint unrestricted certificates or loop back into the router.
7. Stop a preview with a primary wildcard route present; neither its base host nor descendants fall through to primary. Service/tenant namespace collisions are rejected or explicitly resolved.
8. A logged-in primary and preview cannot overwrite/leak authentication cookies across the selected hostname scheme. Test both directions and tenant cases.
9. Migrations/tests/reset in A leave B and primary data unchanged. Test worker/cache/upload scratch, not only primary SQLite.
10. Concurrent allocation/update, foreign listeners, recycled PIDs, duplicate labels, and stale route owners never lead to takeover, cross-routing, or foreign-process signals.
11. Crash during allocation/start/restart/stop; retry reconciles only owned resources. Repeated stop/destroy is safe; malformed state fails closed.
12. Occupied 80/443 produces actionable failure with no silent high-port fallback, automatic incumbent shutdown, or trust changes.
13. Optional Tailscale sharing uses distinct standard-port remote URLs from an authorized separate device; unshare affects only the owned service and preserves preexisting tailnet configuration. LAN tests prove that shared A is reachable while unshared B is not, including unknown peers and enumeration. mDNS resolution is checked separately from TLS trust.
14. Teardown preserves source, primary DBs, other environments, shared-router uptime, and unrelated machine state. No GUI/clipboard/audio side effects.
15. macOS/Linux behavior is tested on actual supported hosts before claiming parity. Strict isolation reports distinguish verified/private, weaker/shared, unsupported, and unknown surfaces.
16. Workspace inventory/groups/explicit targets identify the correct source and environment; project tasks run in that context and preserve argv/exit codes. Finite initialization and repeatable preparation do not accidentally restore another workspace's data.
17. Sharing disabled requires no Tailscale, account login, LAN listener, or mDNS process. Missing optional tools cause an actionable failure only for the requested feature, not ordinary development.

Record versions, resolved context, generation, source/config identity, baseline identity, probe/task results, and caveats. Editing inputs during a test invalidates evidence for the new state; a live before/after fingerprint is weaker than an immutable snapshot. A green port probe is not full isolation proof.

## 16. Decision record and remaining questions

The user's responses resolve these product directions; do not re-ask them as unsettled choices.

| Topic | Recorded direction |
|---|---|
| Product identity | Parcela: workspace/environment organizer, not just a router |
| Implementation | Rust control plane; managed external Caddy and Process Compose; abandon the untested Go drafts |
| URL layout | Favor cookie-safe flat names; configurable nested alternative; tenants off by default |
| Entry point | Our CLI wraps and organizes Process Compose; standalone portless-style run retained |
| Distribution | One-install routing/control experience recommended and explained; backend packaging remains a technical spike, not an assumed user decision |
| Feature scope | Core rb-portless-style behavior plus workspace/Process Compose organization, with explicit exclusions below |
| First POC | Shirabe, Rails + SQLite, full service/environment flow |
| Data initialization | Commands/policy belong to each project, not a universal fresh/clone choice |
| Tailscale | Optional feature desired; prerequisites must not affect unshared environments |
| LAN | Desired optional sharing; mDNS is a proposed optional discovery convenience |
| Public tunnels | ngrok not needed; public Funnel excluded from current scope |

### Follow-up decisions

| ID | Remaining question | Recommendation |
|---|---|---|
| Q1 | Name selected | Parcela; GitHub repository creation authorized. Broader trademark/package clearance remains unverified |
| F1 | Should the CLI optionally create/remove jj/Git workspaces, or only discover/adopt existing ones? | Always support adoption; optional creation delegates to installed VCS, defaults new work to main, never edits source history; source removal is separate and explicit |
| F2 | One project YAML with a native Process Compose subsection, or a small organizer config referencing an existing Process Compose YAML? | One maintained file for a new project; support referencing an existing graph when adopting a project |
| F3 | Should `up` normally attach to the Process Compose TUI, stream logs, or detach? | Interactive attach for humans with explicit detach for agents/always-on; configurable default |
| F4 | Which OS is the first always-on target: Linux or macOS? | macOS local POC plus Linux design; choose the server before platform-specific service acceptance |
| F5 | During migration, must untouched rb-portless projects retain their current no-port URLs? | Explicit front-door cutover; if simultaneous compatibility is essential, a separately verified import/coexistence contract |

Technical questions to settle by experiment, not by making the user design internals: managed backend packaging, exact single-file schema/pass-through compatibility, route reconciliation across Process Compose restarts, standard-port privilege bootstrap, and safe optional LAN listener changes. Configuration examples must be validated against the actual selected versions before calling them executable.

Implementation readiness requires the remaining product answers, a fixed v1 contract, verified packaging/listener/bootstrap strategy, a reviewed Shirabe recipe, and executable acceptance fixtures. A written spec or a name choice is not evidence that the runtime is installed or tested.

## 17. References

- [Prior-art research](research.md) and [source inventory](research-sources.json).
- [Upstream portless](https://github.com/vercel-labs/portless).
- [Process Compose configuration](https://f1bonacc1.github.io/process-compose/configuration/) and [client](https://f1bonacc1.github.io/process-compose/client/).
- [Caddy automatic HTTPS](https://caddyserver.com/docs/automatic-https), [admin API](https://caddyserver.com/docs/api), and [global options](https://caddyserver.com/docs/caddyfile/options).
- [Tailscale Services](https://tailscale.com/kb/1552/tailscale-services), [Serve](https://tailscale.com/kb/1312/serve), and [HTTPS](https://tailscale.com/kb/1153/enabling-https).

The original research remains a historical comparison, not a current recommendation to fork Isola or extend rb-portless. No claim of absent alternatives or runtime reliability follows from a source/documentation review.
