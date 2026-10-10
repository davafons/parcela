# Parcela

A service-agnostic workspace and local-environment organizer, separate from
rb-portless. The selected architecture is **Rust** for the CLI/control plane,
**managed Caddy** for HTTP/TLS, and **managed Process Compose** for supervision.
Project configuration owns commands and data policy; Parcela does not rebuild
either backend.

Target URLs: primary `https://shirabe.localhost`, preview
`https://auth-shirabe.localhost`. Flat names, tenants off, loopback only and
standard HTTPS ports are the defaults. No silent high-port fallback.

## Current implementation

The early Go drafts have been discarded. The Rust foundation implements:

- Strict `parcela.yaml` validation, project-defined task argv and native graph data.
- Explicit adoption of existing checkouts; stable UUIDs independent of branches.
- Private, transactional SQLite registry with project/hostname/primary ownership.
- Project-scoped or explicit machine-wide inventory, including missing sources.
- JSON context with **planned** URLs and explicit capability/verification caveats.

**It does not yet start applications, allocate endpoints, execute tasks, manage
Caddy/Process Compose, install/trust certificates, or share anything.** Adoption
registers a checkout, not a running or data-isolated environment. `doctor` reports
implementation capabilities only; it does not probe installed backends.

## Build and check

Requires Rust 1.89+ and a C compiler for bundled SQLite; tested locally on macOS
with Rust 1.95.0. No Go toolchain, Caddy or Process Compose installation is needed
for this foundation. A distribution license has not yet been selected.

```sh
cargo build --locked
scripts/check
cargo run --quiet -- --path examples/generic config check
cargo run --quiet -- doctor
```

The check script runs formatting, Clippy, tests, the example-schema check and a
Go-artifact check. CI defines macOS/Linux jobs; CI is not live-service acceptance.

## Try adoption without touching your projects

All currently implemented commands output JSON. Errors go to stderr. Use a
private temporary state directory for experiments:

```sh
demo=$(mktemp -d)
mkdir "$demo/checkout"
cp examples/generic/parcela.yaml "$demo/checkout/"
cargo run --quiet -- --path "$demo/checkout" --state-dir "$demo/state" \
  workspace adopt --role preview --label auth
cargo run --quiet -- --path "$demo/checkout" --state-dir "$demo/state" context
cargo run --quiet -- --state-dir "$demo/state" workspace list --all
```

`planned_urls.web` will be `https://auth-demo.localhost`; it is **not serving**.
No runtime/data directory is created, and no source/VCS history is changed.
Repeated adoption with the same role/label preserves identity; conflicts fail
rather than taking over ownership. Primary designation is always explicit.

State defaults to `PARCELA_STATE_DIR`, then `$XDG_STATE_HOME/parcela`, then
`~/.local/state/parcela`; `--state-dir` overrides them. Existing state must be
user-owned and private. Inspection does not create absent state.

## Documentation

- [PRD](docs/PRD.md): release requirements, not a claim of implemented features.
- [Implementation status](docs/implementation.md): verified coverage and next milestones.
- [Configuration and CLI](docs/configuration.md): the current foundation contract.
- [Rust architecture](docs/architecture.md): ownership and managed backend boundaries.
