# Implementation and verification

Parcela is a Rust control plane with external Caddy and Process Compose. Project configuration owns commands and data policy. This replaces the abandoned Go drafts.

## Implemented

- Strict manifest validation, immutable identities, private SQLite registry, explicit adoption and inventory/context.
- Schema 1 -> 2 migration, generation/nonce-protected session and endpoint leases.
- Foreground `proxy run/status`, literal `run`, native `up`, finite `task`, recorded `status`, optional version probes in `doctor`.
- Dedicated Caddy config/storage/admin socket; serialized exact-host route updates, internal CA, trust installation disabled by default.
- Private runtime/locks, retained child groups, owned listener ancestry, initial/re-established readiness, route revocation before normal teardown.
- Native Process Compose graph export, explicit private Unix-socket mode, no dotenv/watch/TUI.
- Opt-in project-owned Shirabe Rails integration, preserving primary data while binding previews to private paths and secrets.
- Explicit macOS operator helper for guarded PF standard-port forwarding, a root-owned forwarding boot job, and CA trust. Not an ordinary-command side effect.

## Verified locally, macOS, 2026-10-10

Rust/Cargo 1.95.0, Caddy 2.11.7, Process Compose 1.122.0, Ruby 4.0.2 / Rails 8.1.3.1.

`scripts/check` passes formatting, Clippy, 25 Rust tests, generic schema validation and the Go-artifact guard. Coverage includes identities/concurrency, private state/symlinks, malformed state, schema migration, leases/generations, config ownership, reserved context and explicit forwarded-port URL generation.

`scripts/check-live` passes disposable high-port acceptance for two TLS routes using an explicit disposable CA, literal argv, forwarded-header spoof rejection, SSE, WebSocket upgrade, unknown-host rejection, duplicate ownership, SIGTERM/exit status, foreign-port preservation, a native web+worker graph and independent teardown.

`scripts/check-shirabe --process-compose ...` passes with two bounded tracked-source snapshots, fresh schemas and a synthetic user, HTTPS `/up`, exact host authorization, CSRF origin rejection and valid state-changing requests, canonical route/mailer URLs, Cable origin configuration, actual Authentication cookies, separate signing secrets/databases, cross-environment cookie rejection, native CSS/jobs running, independent teardown, and restart retaining private data/auth. No dictionary corpus, uploads, secrets, caches or dependency trees are copied. Input source hashes are checked after acceptance.

Shirabe's targeted `bin/test` passes 8 tests / 25 assertions for the integration/launcher and production host authorization. Targeted RuboCop, ShellCheck, Bundler dependency checks and house rules pass.

## Real-machine cutover state

User-authorized user-local binaries and the unprivileged `org.parcela.proxy` launch agent are installed. The real Shirabe checkout is adopted as primary; its launcher/manifest use Parcela and Process Compose, without the rb-portless gem/binstub/Procfile. Its existing development databases and uploads remain in `storage`; no preparation/reset/import was run.

The user retired the shared rb-portless front door, accepting loss of Nadeshiko routing, and authorized PF forwarding, the boot job and System-keychain CA trust. The operator setup helper completed successfully. `parcela proxy status` verifies the owned admin endpoint and standard-port forwarding. After the user started `bin/dev`, `/usr/bin/curl -q --noproxy '*' -D - https://shirabe.localhost/up` returns HTTP/2 200 with system trust, without a custom CA or hostname override. HTTP port 80 redirects the same path to HTTPS with status 308. Process Compose's `process list --output json` over the environment's private `pc.sock` reports web, CSS, jobs and SDK as Running. This proves startup and the health route, not full application or watcher acceptance.

`launchctl print system/org.parcela.pf` shows the forwarding job loaded with last exit code 0. The installed privileged helper matches `scripts/setup_macos.py`. Reboot persistence and browser trust remain unverified. No primary application services, jobs, migrations or imports were launched during this host setup.

## Gaps

No cross-process stop/crash recovery, detached application control, alias/tenant routing, sharing, general data-isolation certification, automatic backend packaging or source/data destruction. Daemonized descendants and failed route revocation are not magically reconciled; uncertain leases remain recorded. HTTP readiness is not continuous after listener establishment.

Actual Shirabe Cable transport, linked SDK rebuild behavior, full application/searchable-corpus regression, broader application HTTP/2 behavior, Linux runtime, remote CI, release packaging, browser trust and reboot persistence remain unverified. The privileged PF/trust helper succeeded on this host; custom/conflicting PF ruleset handling has only nonprivileged guard checks. The PRD's P0 goals are not a shipped-feature list.
