# Implementation and verification

Name selected: Parcela. Implementation authorized 2026-10-10. Rust restart
selected by the user; this status supersedes the unfinished Go drafts.

## Architecture choices

- Rust CLI/control plane; managed external Caddy for HTTP/TLS and Process Compose
  for supervision. No embedded Go backend or parallel Go implementation.
- One maintained `parcela.yaml`; organizer fields parsed strictly, native Process
  Compose graph retained as data. See [current schema](configuration.md).
- Existing checkouts adopted first; source creation/removal remains separate.
  Future optional creation delegates to jj/Git and starts from main by default.
- Planned: one Process Compose instance per environment with private Unix socket
  and explicit socket mode. Interactive attach may be supported; agents use
  explicit noninteractive foreground/detached mode.
- Standard-port router setup, CA trust, boot and network sharing are explicit
  authorized operations. Backend packaging/version policy remains a spike.
- macOS local verification first; Linux CI is not actual Linux service acceptance.

See [Rust architecture](architecture.md) for implemented and future boundaries.

## Milestones

1. [x] Initialize jj/GitHub, preserve PRD and settle Rust/backend architecture.
2. [ ] Foundation and resource allocation:
   - [x] Cargo binary/library, lockfile, Rust check script and CI workflow.
   - [x] Strict configuration/task argv validation and native graph data.
   - [x] Stable identities, private transactional registry and ownership conflicts.
   - [x] Explicit adoption, scoped/all inventory and read-only planned context.
   - [ ] Actual jj/Git workspace discovery/current metadata and validated rebind.
   - [ ] Endpoint leases, resource bindings/private runtime creation and generations.
3. [ ] Managed Caddy packaging/config/routes/TLS, ownership and safe startup.
4. [ ] Standalone run, Process Compose lifecycle/tasks/logs/restarts/reconciliation.
5. [ ] Shirabe Rails + SQLite disposable two-environment acceptance.
6. [ ] Optional sharing/boot/cleanup coverage and explicit capability reporting.
7. [ ] Complete release acceptance, supported-host checks and remote CI verification.

## Verified Rust foundation, 2026-10-10

Local macOS, Rust/Cargo 1.95.0. `scripts/check` includes:

- `cargo fmt --check`.
- `cargo clippy --locked --all-targets -- -D warnings`.
- `cargo test --locked` (17 foundation integration tests).
- Generic manifest validation through the CLI.
- Repository Go-artifact check, excluding build/VCS internals.

`cargo build --locked`, CLI help and the static `doctor` report also passed.
The two concurrent-adoption tests passed ten additional consecutive runs.

Coverage: strict schema/names/tasks, native graph field retention, flat planned
URLs, UUID stability and jj-first metadata detection, source canonicalization,
reopen/idempotence, rollback and global hostname/primary conflicts, concurrent
CLI adoption, private state permissions and symlink rejection, malformed/future
registry rejection, missing source retention, JSON/error separation, explicit
capability caveats and unchanged source files. Tests use temporary directories
and isolated state. They launch only the Parcela CLI, not application services.

## Explicitly unverified / unsupported

No runtime endpoint allocation, live Caddy/Process Compose management, executable
graph export, preparation/tasks, process ownership, readiness, stop/restart,
certificate/trust/bootstrap, LAN/mDNS/Tailscale, boot or data reset/destroy.
`doctor` is a static capability report, not an installed-backend diagnostic.
No services, trust stores, existing listeners or tailnet settings were changed.
Planned URLs are not live HTTPS acceptance and adoption is not data isolation.

Real jj/Git discovery and branchless/detached workflows are not proven by marker
fixture tests. The configured CI workflow has not been run remotely in this
change; Linux service behavior, minimum Rust toolchain and release packaging
remain unverified. A license must be chosen before release/importing code.
Historical Ruby pilot evidence remains in the PRD, not Rust runtime evidence.
