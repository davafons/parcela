# Parallel local environments: existing tools and recommendation

Research date: **2026-10-10, 14:53 JST (Asia/Tokyo)**.

**Historical recommendation:** this report records the initial adopt-first investigation. The subsequent decision is a separate workspace-aware routing project, preserving rb-portless's upstream-port purpose and using Process Compose for orchestration. See the [replacement PRD](PRD.md) for current scope and questions. Runtime/installation statements below describe the original research stage, not the later disposable pilot.

## 1. Executive conclusion

**Yes: several open-source GitHub projects already implement almost exactly the requested idea.** Some manage native services, clone databases, allocate ports, handle Compose stacks, and expose agent-facing CLIs or MCP servers—not just URL proxies.

Closest matches: **Isola, Ecluse, Devbay, Git Treeline, and workz**. **devenv** is also a serious contender: its current documentation includes a native process manager, automatic ports, readiness checks, socket activation, and named HTTP/HTTPS endpoints. Do not dismiss it as just a package manager.

Recommendation:

- **Native-first:** trial Isola first; investigate contributing jj/workspace identity support rather than rebuilding it.
- **Hybrid:** trial Ecluse, explicitly testing ownership, env inheritance, and readiness limitations.
- **Container-first verification:** trial Devbay. Its task subgraphs and structured test results already overlap much of a possible Multidev PRD.
- **Already using Nix/devenv:** evaluate its built-in functionality and worktree extensions before adding another orchestrator.

A new project is only justified if the practical workflow still needs this combination:

1. First-class **jj workspaces, detached Git worktrees, and arbitrary checkout paths**, without branch names as identity.
2. Existing native/hybrid workflows without requiring containerizing the app.
3. Ownership-safe lifecycle and concurrent-agent coordination across repositories.
4. Explicit guarantees about mutable state, not “a different port means isolated.”
5. Verification reports tied to the actual code/config/data baseline tested.

This review did **not** establish that existing tools cannot be extended to provide those features. The resulting [original PRD](PRD-0.1.md) was a conditional gap-focused draft. It has since been superseded by the [current product PRD](PRD.md), reflecting the user's separate-project decision rather than a claim that alternatives are absent.

## 2. Scope, method, and limitations

### Desired workflow

An agent already has a checkout created by Herdr, Git worktrees, or jj. It should boot that checkout's app, workers, asset servers, databases, caches, and dependencies; obtain addresses without hand-editing ports; run independent tests; leave a review URL and evidence; and stop/delete only its own runtime resources.

“Copy the environment” means reuse the declared toolchain and safe baseline data, not blindly copy live database files, runtime directories, credentials, or shared writable caches.

### Investigation

- Ten GitHub repository-search queries covering worktree isolation, ports/databases, Compose, agents, jj/Jujutsu, and devenv. Queries included `worktree environment isolation`, `worktree ports database`, `worktree docker compose`, `jj workspace environment`, and `devenv worktree`.
- Repository documentation for **26 projects**; source-archive searches for jj integration in Isola, Ecluse, Devbay, Treeline, and falq, with targeted worktree/process-ownership code review in Isola.
- Current Docker Compose, PostgreSQL, Redis, SQLite, and devenv documentation.
- Existing local `~/workspace/rb-portless` README, project instructions, and module layout.
- Configured web search/extraction failed because `MONID_API_KEY` was absent. Research instead used the public GitHub API, raw repository files, source archives, and direct HTTPS documentation. No browser UI was needed.

This is broad prior-art research, not an exhaustive proof of absence. Documentation establishes **documented capability**, not runtime reliability. No installation scripts or downloaded code were executed; no live isolation, load, or security tests were performed. Promotional benchmarks and star counts were not used as quality scores.

## 3. Closest alternatives

**Not documented** means inspected sources did not establish support, not that support is impossible. No jj support was found in the reviewed source/docs of the five deeply inspected tools; none was runtime-tested with jj.

| Tool | Runtime approach | Documented isolation/control | Main consideration |
|---|---|---|---|
| [Isola](https://github.com/cyucelen/isola) | Native arbitrary commands; existing DB/cache servers | Ports, HTTP/HTTPS proxy, env references, setup, Postgres template clones, Redis logical DBs, logs, CLI/TUI | Closest native match. Git/branch identity; does not manage private DB server instances |
| [Ecluse](https://github.com/hefgi/ecluse) | Host, container, hybrid | Worktree adoption, service ports/env, Compose projects/volumes, native spawning, CLI/JSON | Closest hybrid match. Documents check-to-bind race and spawn/kill-only management |
| [Devbay](https://github.com/Clarittyai/devbay) | Container runtime required; tasks may need no containers | Origins/data/ports, health probes, task dependency subgraphs, seed templates, typed test failures, CLI/MCP, budgets | Closest verification product. Git-based and container-oriented |
| [Git Treeline](https://github.com/git-treeline/cli) | Native app supervisor; existing backing services | Machine-wide allocation, Postgres/SQLite clones, test DBs, Redis namespaces, HTTPS, start/await/stop/restart, JSON/MCP | Very close CLI. Git assumptions; TCP-ready is not end-to-end readiness |
| [workz](https://github.com/rohansx/workz) | Provisioning plus detached native dev command; optional Compose | Port ranges, Postgres create/clone and Docker fallback, service ports, logs, preview, CLI/MCP | Broad overlap. Cache symlinks and port-based reaping need safety review |
| [falq](https://github.com/sunduq-ai/falq) | Stack-neutral manifest provisioning/command wrapper | Slots/port blocks, env/config generation, hooks, Compose naming/linting, agent context, JSON diagnostics | Good configuration layer; deliberately not a background supervisor/control plane |
| [devenv](https://github.com/cachix/devenv) | Nix-managed native toolchain/services/tasks | Automatic ports, dependency/restart/readiness, socket activation, local proxy, profiles, lifecycle testing | Strong foundation if Nix is acceptable; checkout/data identity still needs configuration |

### 3.1 Isola: first native candidate

[README](https://github.com/cyucelen/isola/blob/main/README.md) · [Configuration](https://github.com/cyucelen/isola/blob/main/docs/configuration.md) · [Accessory extension guide](https://github.com/cyucelen/isola/blob/main/docs/writing-an-accessory.md)

Isola describes the same problem: parallel agents need independent running environments, not just source directories. Services are arbitrary commands, including workers without ports. Setup steps install dependencies or run migrations. Env values reference sibling service URLs/accessory connection strings. It includes local routing, optional TLS, logs, dashboard, and agent skills.

Its Postgres accessory creates a per-worktree database on an **existing** server and can clone a template. Redis allocates numbered logical DBs on an existing server. Neither starts/isolates the server itself. Additional accessories are an intended extension point.

Targeted source findings:

- [`internal/git/worktree.go`](https://github.com/cyucelen/isola/blob/1a049b53dbeaeddcf80e4c44fdc4f29066e96646/internal/git/worktree.go) discovers checkouts with `git worktree list --porcelain` and derives routing identity from branch names. jj workspaces need not appear as Git linked worktrees or have named branches.
- [`internal/process/runner.go`](https://github.com/cyucelen/isola/blob/1a049b53dbeaeddcf80e4c44fdc4f29066e96646/internal/process/runner.go) starts process groups. `StopPID` derives a group from a stored PID and signals it; the helper itself does not validate a stored process birth identity. This is a risk to investigate, **not** a demonstrated cross-environment kill or complete caller audit.
- Redis logical DBs do not contain `FLUSHALL`, global configuration, or all messaging behavior. A private instance is safer for destructive verification.

**Verdict:** likely lowest-cost native path if jj identity and cleanup guarantees can be addressed.

### 3.2 Ecluse: hybrid environments already exist

[README and known limits](https://github.com/hefgi/ecluse/blob/bd4c575a78f65e470908a94b5047b456be3523fa/README.md)

Offers `container`, `host`, and `hybrid` modes; adopts externally created worktrees; writes env/ports; launches native apps; gives Docker dependencies private projects/volumes; exposes CLI/JSON and actual-vs-expected port discovery.

Documented limitations:

- Ports are **checked, not reserved through bind**; an external process can win the gap.
- Native management is spawn/kill through tmux or a nohup fallback, not continuous supervision/restart.
- Apps must consume injected ports; hardcoded config is not rewritten.
- Database initialization/migration are hooks, not a generic clone lifecycle.
- `.env`/`.env.local` inheritance defaults to symlinking, with copy/opt-out controls. Review before agents edit local env files.

**Verdict:** evaluate extension rather than inventing the hybrid model again.

### 3.3 Devbay: verification is already a product feature

[README](https://github.com/Clarittyai/devbay/blob/main/README.md) · [Capabilities/limits](https://github.com/Clarittyai/devbay/blob/fedc2aa42fc6fb603a7a7c2d511f17212d6afd53/docs/CAPABILITIES.md) · [Acceptance scenarios](https://github.com/Clarittyai/devbay/blob/main/docs/ACCEPTANCE.md)

Frames the bottleneck as **verification**. Documents explicit tasks/dependency subgraphs, health probes, datastore templates, separate browser origins, typed test results, argv-based commands, approval gates, secret references/scrubbing, CLI/MCP, and cooling older bays when a budget is exceeded.

This weakens any proposed differentiation that is simply “agents create environments and return structured test failures.” Devbay already does that.

Trade-offs:

- Container runtime required; long-running services run in containers.
- Git-worktree-based creation; no jj-native identity established.
- No background orchestration daemon; budget/lifecycle decisions happen during commands.
- One repo and machine per bay.
- Approval, egress, scrubbing, and acceptance claims were read, not security-tested. An argv array is not a sandbox: an approved executable can itself execute arbitrary code.

**Verdict:** strongest adopt-first candidate if containerizing the app is acceptable.

### 3.4 Treeline and workz: more than port allocation

[Treeline README](https://github.com/git-treeline/cli/blob/d3bedcfc272a86fb1f6afb4ebe74278f9aa02dd2/README.md) documents machine-level allocation, app supervision with `start --await`, database clones/auxiliary test DBs, Redis allocation, routing, JSON/MCP and review worktrees. Configuration examples include Node/frontend/Rails. Await is TCP readiness, not proof the app/browser route works.

[workz README](https://github.com/rohansx/workz/blob/main/README.md) includes native dev-server detection/overrides, detached run/logs/stop, preview, named service ports, Postgres create/template clone, and Compose integration. It also describes default cache symlinks and reaping processes on allocated ports. Those conveniences need stronger mutable-cache/ownership policies for this safety goal. A port registry entry does not prove its current listener is the process originally launched.

**Verdict:** include if Isola's model proves awkward. Do not build another inventory/CLI just because rb-portless lacks environment-level management.

## 4. Additional prior art

### Compose-oriented tools

- [worktree-compose](https://github.com/mostafasudo/worktree-compose): per-worktree Compose projects/ports/env, CLI/MCP. Despite “zero-config” wording, ports must be parameterized; README documents clustered-port collisions and a configurable stride. Headline is not a formal guarantee.
- [Port](https://github.com/jdtzmn/port): Compose isolation with Traefik branch domains, removing conflicting container host binds. Also has `port run` for native processes—**not exclusively container-only**. Not a general data-baseline manager.
- [Treehouse](https://github.com/Geocodio/treehouse): Compose stacks, machine-wide port claims, dependency clones, provisioning and GC. Maintainers call it an internal tool without a commitment beyond their own use cases.
- [Berth](https://github.com/zoltanersek/berth): Git/Compose environments, CLI/dashboard, snapshots/reset and agent hooks. Snapshotting briefly stops containers; not an instant live copy or native runtime solution.
- [wtpool](https://github.com/thousand-ships/wtpool): provisioning rather than supervision; port-block registry, templates, SQLite/Postgres/volume seeds, adoption. Useful minimal-layer inspiration.

Compose itself is service/language agnostic. Unique project names, private networks/volumes and dynamic loopback host ports provide much of the desired isolation. Caveats: fixed `container_name`, shared explicit volume names, external networks/volumes, host networking, absolute bind mounts, Docker socket access and fixed published ports. Project naming alone does not fix these.

Sources: [project names](https://docs.docker.com/compose/how-tos/project-name/) · [networking](https://docs.docker.com/compose/how-tos/networking/).

### Native/Nix infrastructure and Herdr extensions

- [Process Compose](https://github.com/F1bonacc1/process-compose): neutral native orchestration, dependencies, readiness/liveness, recovery, logs, CLI/TUI/API/MCP. Could be a backend; not itself checkout/database allocation policy.
- [devenv process docs](https://devenv.sh/processes/): native manager, named endpoints, auto ports, socket activation. Holds allocated sockets during evaluation and releases them before ordinary child binding; activation is stronger for compatible apps. Explicit per-checkout data configuration remains necessary.
- [devenv-lazy-cow-worktrees](https://github.com/onnimonni/devenv-lazy-cow-worktrees): unusually close. On-demand services/HTTPS, Postgres clones/checkout roles, private Redis servers, CoW checkouts, migrations and lifecycle. Git wrappers and GitHub synchronization/removal are not neutral jj behavior. macOS uses a volatile APFS RAM-disk Postgres 18+ setup. Its notes explicitly say SQL-proxy boundaries do not protect against arbitrary same-user programs. Valuable algorithms, not an automatic drop-in.
- [herdr-devenv-worktree](https://github.com/sample-usr/herdr-devenv-worktree): Herdr plugin for devenv worktree setup; relevant if choosing Nix.
- [devenv-agents](https://github.com/wyattjoh/devenv-agents): `project` CLI/Herdr plugin for checkout/bootstrap/adoption/lifecycle. Not established full runtime-data isolation by itself.
- [Devbox](https://github.com/jetify-com/devbox): reproducible Nix shells/toolchains. Toolchain isolation is not mutable-data/network isolation.
- [Croft](https://github.com/nsrosenqvist/croft): recipes, Compose passthrough, hooks/watch and worktree port isolation; smaller wrapper rather than established complete resource lifecycle.

### Checkout managers and narrower products

- [Worktrunk](https://github.com/max-sixty/worktrunk): Git worktree workflows/hooks, CoW ignored files, hash-port template. Integration point, not full runtime isolation; hashing alone is not exclusive global allocation.
- [agent-worktree](https://github.com/nekocode/agent-worktree): checkout/agent/merge lifecycle and setup hooks, not established independent DBs/runtime supervision.
- [Treq](https://github.com/tznc/treq): jj-backed agent development environment/stacked workspaces. Proof jj agent tooling exists; inspected docs do not establish the requested runtime layer.
- [ldev](https://github.com/berdon/ldev): service registry, port/resource leases, Traefik, CLI/REST/MCP. Control-plane prior art, not documented whole-app/data cloning.
- [Portree](https://github.com/fairy-pitta/portree): Isola's upstream; native worktree services/ports/proxy without accessory lifecycle.
- [portless](https://github.com/vercel-labs/portless): URL/proxy/process wrapper and worktree-aware naming, not whole mutable-state management.
- [DDEV](https://github.com/ddev/ddev): established Docker local environments, HTTPS, DB snapshots/import and add-ons; PHP-focused conventions differ from the requested neutral core.
- [DevPod](https://github.com/loft-sh/devpod): devcontainer local/remote workspaces. Relevant if a whole container/VM workspace is preferable to lightweight native orchestration.

## 5. rb-portless: reuse the routing layer

Local project: `~/workspace/rb-portless`, not `../rb-portless` (absent from this workspace).

Already solves HTTP/HTTPS/WebSocket routing, local TLS, loopback defaults, random app ports/env injection, command supervision, route registry/locking, Git-worktree naming, tenant wildcards and monorepo commands.

Missing at environment scope: DB/cache/storage/queue resource ownership; clone/reset/versioned baselines; independent dev/test DBs; dependency/migration/readiness graph; jj identity independent of branches; source-bound verification records.

**Protocol distinction:** an rb-portless `alias` for a Postgres port does not make its HTTP reverse proxy a Postgres/TCP proxy. Databases need direct distinct ports, private networks, Unix sockets, or a protocol-aware proxy. Same for Redis.

Prefer an optional routing adapter over expanding rb-portless into the whole orchestrator. Review proxy cleanup authority separately; stopping one environment must not stop the shared proxy for everyone.

## 6. Isolation hazards every solution must confront

1. **Ports do not isolate cookies.** Cookies use host/domain, not port. Distinct hostnames help, but parent-domain cookies can span sibling environments. Use host-only or environment-scoped cookie domains; test tenant subdomains.
2. **Postgres templates need quiescence.** `CREATE DATABASE ... TEMPLATE` fails with active source connections. Never terminate live dev sessions to clone. Use a prepared immutable template or consistent dump/restore. [Postgres docs](https://www.postgresql.org/docs/current/manage-ag-templatedbs.html).
3. **Live SQLite is not an ordinary file copy.** Use backup API/quiescent snapshots; copying only the main file during WAL activity can omit state. [SQLite backup](https://www.sqlite.org/backup.html).
4. **Redis logical DBs are weaker isolation.** Shared persistence/global operations remain; Cluster supports only DB 0. Prefer private instances for destructive tests. [Redis SELECT](https://redis.io/docs/latest/commands/select/).
5. **Writable caches reintroduce interference.** Dependencies, build outputs, tmp/PID/cache files and uploads must be private or safely CoW. Reuse immutable downloads, not arbitrary writable runtime trees.
6. **Port/PID alone does not prove ownership.** Both are reused. Validate process/resource identity before cleanup; never kill foreign listeners.
7. **`.localhost` is client-dependent.** Browser behavior is not universal system DNS. Separate host-loopback, container-private and browser-routed endpoints; test real clients/TLS.
8. **External side effects remain shared.** SMTP, payments, object storage, webhooks and test accounts need mocks, separate credentials/namespaces or explicit sharing.
9. **Native isolation is not a security sandbox.** Cooperating processes can avoid collisions; arbitrary same-user code can still access host files/processes. Containers/VMs need a separate threat model.

## 7. Adoption/contribution decision gate

Before implementing, evaluate in disposable directories without production credentials or system-wide GUI input:

- Rails + Postgres + worker + Redis + tenant hosts + test DB.
- Node frontend/API + Postgres + HMR/WebSocket + browser auth.
- Go or Python service to prove the neutral contract.

Trial Isola/Ecluse for native/hybrid; Devbay for containers; devenv if Nix is acceptable. Document unsupported modes rather than silently changing scenarios.

Acceptance exercises:

- Adopt Git and jj checkouts without source/bookmark/branch mutation.
- Boot three concurrently, including identical branch labels across repos and bookmarkless/detached workspaces.
- Migrate/reset one; preserve the others' data/uptime and primary environment.
- Preserve foreign listeners/env files during failures/teardown.
- Check DB/jobs/uploads/cookies/tenants/TLS/HMR/browser flows, not just TCP listening.
- Interrupt startup/migrations and retry; stop twice; remove a checkout while retaining its runtime; simulate stale ownership.
- Measure boot/memory/disk/failures/orphans/setup effort.

**Adopt** if normal configuration passes. **Contribute/wrap** if the gap is a contained jj/identity/routing/report adapter. **Build** only if architectural gaps remain and maintaining another lifecycle/resource manager is justified. This research is not that acceptance exercise.

## 8. Source snapshots

| Repository | HEAD recorded at review |
|---|---|
| Isola | `1a049b53dbeaeddcf80e4c44fdc4f29066e96646` |
| Ecluse | `bd4c575a78f65e470908a94b5047b456be3523fa` |
| Devbay | `fedc2aa42fc6fb603a7a7c2d511f17212d6afd53` |
| Treeline | `d3bedcfc272a86fb1f6afb4ebe74278f9aa02dd2` |
| falq | `32c39b5e2f1ca65bb80e79fed4c4f769428788f6` |

Archives were fetched from default branches shortly before recording heads. Other links target moving docs/branches; observations are as of retrieval. [Inventory](research-sources.json) records all reviewed repos/metadata. No complete license audit, release-maturity assessment, or runtime certification is claimed.
