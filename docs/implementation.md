# Implementation and verification

Parcela is a Rust control plane with external Caddy and Process Compose. Project configuration owns commands and data policy. This replaces the abandoned Go drafts.

## Implemented

- Strict manifest validation, immutable identities, private SQLite registry, explicit adoption and inventory/context.
- Schema 1 -> 2 migration, generation/nonce-protected session and endpoint leases.
- Foreground `proxy run/status`, literal `run`, native `up`, finite `task`, live `status`, optional version probes in `doctor`.
- Dedicated Caddy config/storage/admin socket; serialized exact-host route updates, internal CA, trust installation disabled by default.
- Private runtime/locks, retained child groups, owned listener ancestry, initial/re-established readiness, route revocation before normal teardown.
- Native Process Compose graph export, explicit private Unix-socket mode, no dotenv/watch/TUI.
- Live-owner `down`/`stop`, whole-graph `restart`, explicit `up --detach`, service/log inspection, reserved `url`, UUID targeting and readable tables.
- Read-only sibling jj/Git discovery with path/revision/ref/adoption metadata. Discovery never adopts or changes source/history.
- Opt-in project-owned Shirabe Rails integration, preserving primary data while binding previews to private paths and secrets.
- Explicit macOS operator helper for guarded PF standard-port forwarding, a root-owned forwarding boot job, and CA trust. Not an ordinary-command side effect.

## Verified locally, macOS, 2026-10-10

Rust/Cargo 1.95.0, Caddy 2.11.7, Process Compose 1.122.0, Ruby 4.0.2 / Rails 8.1.3.1.

`scripts/check` passes formatting, Clippy, 25 Rust tests, generic schema validation and the Go-artifact guard. Coverage includes identities/concurrency, private state/symlinks, malformed state, schema migration, leases/generations, config ownership, reserved context and explicit forwarded-port URL generation.

`scripts/check-live` passes disposable high-port acceptance for two TLS routes using an explicit disposable CA, literal argv, forwarded-header spoof rejection, SSE, WebSocket upgrade, unknown-host rejection, duplicate ownership, SIGTERM/exit status, foreign-port preservation, a native web+worker graph and independent teardown.

`scripts/check-shirabe --process-compose ...` passes with two bounded tracked-source snapshots, fresh schemas and a synthetic user, HTTPS `/up`, exact host authorization, CSRF origin rejection and valid state-changing requests, canonical route/mailer URLs, Cable origin configuration, actual Authentication cookies, separate signing secrets/databases, cross-environment cookie rejection, native CSS/jobs running, independent teardown, and restart retaining private data/auth. No dictionary corpus, uploads, secrets, caches or dependency trees are copied. Input source hashes are checked after acceptance.

Shirabe's targeted `bin/test` passes 8 tests / 25 assertions for the integration/launcher and production host authorization. Targeted RuboCop, ShellCheck, Bundler dependency checks and house rules pass.

GitHub Actions [run 38065259625](https://github.com/davafons/parcela/actions/runs/38065259625) passed `scripts/check` on macOS and Ubuntu for runtime commit `06636da42b6c98d93d10c489b83fa1aa1d2c053c`. `gh run view 38065259625 --repo davafons/parcela` reports the result. These are Rust/schema checks, not Linux live-service acceptance.

## Verified on Tower, Linux, 2026-10-12

Rust 1.95.0, Caddy 2.11.7, Process Compose 1.122.0, lsof 4.99.5. Backend downloads/builds reside in a private task directory; no system install, trust change, or network sharing occurred.

The portable-handoff stack's `scripts/check` passes 54 Rust tests and 20 Python destination-acceptance guard tests on macOS and Linux, plus formatting, Clippy and schema validation. The guard suite includes repeated immutable inspection, strict retained-input SQLite sidecar/WAL refusal, stopped-queue fingerprints and opener checks, row/schema/application metadata change detection, private paths, a web/CSS-only graph, explicit unprivileged ports, private diagnostics, and an ASCII-safe Rails probe.

Generic Linux live backend acceptance passes with actual Caddy, Process Compose and lsof. It exercises two simultaneous TLS routes, literal argv, forwarded-header spoof rejection, SSE, WebSocket upgrade, unknown-host rejection, duplicate ownership, SIGTERM/exit status, foreign-port preservation, a native web+worker graph over a private UDS, and independent teardown. Test HTTP 34839 / HTTPS 56943 are not standard-port acceptance.

Paused full-data Shirabe destination acceptance passed as `resume-20261012/acceptance-05` under `/home/davafons/.local/state/parcela/agent-tests/portable-handoff`. Its private report has `complete: true`: named HTTPS health/search returned 200, Rails read 3,659,561 dictionary entries and 414 upload records, Japanese tokenization and a sampled Active Storage download passed, and web/CSS were Running. Queue/mail used test adapters; copied jobs and SDK watching stayed disabled. After shutdown the route returned 503, no queue openers/sidecars remained, the queue fingerprint matched, and all retained database snapshots and upload hashes were unchanged. Foreground application/proxy exits were 143/0. [The repeatable procedure](check-portable-shirabe.md) owns the setup.

The earlier queue failure was an artifact/runtime distinction: Rails switched the copied queue to WAL, changing only journal-mode/counter header fields. Retained snapshots still require rollback-journal format and exact SHA-256 preservation. The stopped runtime queue is compared byte-for-byte except for those fields, without rewriting it; [the transfer contract](transfer.md#destination-shirabe-acceptance) defines that boundary. The regression failed before the fix and passes afterward. Acceptance ran under `LANG=C LC_ALL=C` and verifies the locale-safe Rails probe.

These snapshots are independently consistent, not an atomic application checkpoint. This proves a paused replica's tested service/data paths, not active agent continuation, complete application regression, or execution handoff.

## Local CLI verification, 2026-10-12

The new local management commands passed full `scripts/check` and expanded `scripts/check-live` on macOS and Tower/Linux using disposable source/state. Current checks include 65 Rust tests and 20 Python tests, formatting, Clippy, schema validation and the Go-artifact guard. Rust is 1.95.0, Caddy is 2.11.7 and Process Compose is 1.122.0. jj discovery tests use 0.40.0 on macOS and 0.45.1 on Linux.

The expanded live check proves foreground-owner stop, detached startup/readiness, restart with new generation/nonce, rejection of stale stop requests, independent teardown/data retention, service/log queries, UUID targeting and aggregate status. A fixture stays listening but returns HTTP 503; status reports unhealthy while recorded intent remains ready. Foreign control sockets survive failed startup, and failed detached startup leaves stopped intent. The detached workspace-generation regression failed before the fix and then passed. Rust checks cover missing-source UUID targeting, no-state-creation inspection, malformed health responses, private/foreign sockets and bounded backend output.

Final disposable test ports were HTTPS 49697 / HTTP 49698 on macOS and HTTPS 44077 / HTTP 39811 on Linux. Tower's exact final code and private `check.log`/`live.log` are under `/home/davafons/.local/state/parcela/agent-tests/cli-20261012-01`. This is local lifecycle acceptance, not standard-port setup, complete Shirabe regression, crash reconciliation or active handoff. Installed binaries and primary services were not upgraded or restarted by this work. Independent review has not run.

## Lifecycle recovery verification, 2026-10-13

The receipt-reconciliation checkpoint passes `scripts/check` on macOS and Tower/Linux, with 67 Rust tests and 20 Python tests, formatting, Clippy, schema validation and the Go-artifact guard. Both hosts pass normal `scripts/check-live` and all 14 `scripts/check-recovery` cases with actual Caddy 2.11.7 and Process Compose 1.122.0 in disposable private state. Every recovery fixture verifies its supervised service listeners are closed. Installed binaries, primary services, trust and sharing configuration were unchanged.

The recovery suite covers owner-death route withdrawal and foreign backend-port reuse, seven-second native graceful shutdown, overlapping stop requests, replacement preflight failure preserving the healthy owner, proxy outages preserving two application graphs, lost stop replies, authenticated reattachment without process replay, proxy guardian death with surviving Caddy, invalid native credentials, interrupted stop completion, completed-stop receipt reconciliation, replaced socket and process-birth refusal, and a 12-event HTTPS stream across route-lease renewals. The stream case also probes new requests and compares actual Caddy configuration through its private admin socket. These tests do not certify long-lived connections across configuration changes or guardian restart.

The expanded suite reproduced another defect on both hosts. After controller death during shutdown, Process Compose could finish the graph but leave the durable record at `stopping`. Repeated `down` then failed with `native supervisor birth identity is unavailable; no PID was signaled`. A focused Rust regression failed before the fix and passes afterward. Recovery now completes a matching quiescent stopping record through the existing stopped-receipt path, saving the stopped phase before inode-qualified cleanup and registry reconciliation. It still refuses to infer stopped intent from a dead running graph or to reconcile while a tracked descendant survives. The failing regression is in `nzuoktmz/d0b6ad71`; its fix is in child `zpylxtmy`.

Before/after reports, source archive hashes and both-host final logs are retained under `/Users/davafons/.local/state/parcela/agent-tests/lifecycle-pickup-20261013T004629Z`. Tower artifacts are under the same suffix in `/home/davafons/.local/state/parcela/agent-tests`. Earlier Tower attempts failed because of test invocation/PATH setup and an unlinked custom Cargo target; the corrected final runs passed. The inherited caller-disconnect fix is now verified against current Linux source as well as macOS.

This checkpoint proves the listed local lifecycle and recovery cases, not arbitrary orphan adoption, native exit-code observation after controller death, reboot recovery, or active environment handoff. Ongoing-shutdown retry and contending crash-stop callers were verified in the follow-up below. Independent review remains open.

### Ongoing shutdown and contending stop recovery

The follow-up passes `scripts/check` on macOS and Tower/Linux with 69 Rust and 20 Python tests, normal live acceptance and all 16 real backend recovery cases. One added case retries while the native service is still in its seven-second shutdown after controller death; it passed without a production change. Another forces two stop callers to contend for recovery ownership after a crash. Before the fix, the elected caller succeeded while the contender failed on `owner.lock` on both hosts. A focused Rust regression failed at the same boundary.

Stop recovery now waits for ownership within the recorded shutdown budget, preserving the caller's selected session. It rechecks generation and token while waiting and after acquisition. Same-generation completion returns success without issuing another native command or performing cleanup. Non-stop recovery remains fail-fast on contention; invalid lock files and permission errors are not treated as contention. The live race case verifies both exits are zero, the original generation is stopped, and exactly one native `down` request was made. Rust guards also prove that a waiting stale caller leaves a successor generation unchanged.

The disconnected-caller regression intermittently accepted a reply after its caller socket was dropped. The fixture now explicitly shuts down the caller socket and retains its reply-rejection assertion. Twenty focused repeats passed before the full suite. No control receiver change was needed.

The failing tests are in `wsylmsyp/d04ef09f`; the fix is in child `xnsvmvlv`. Source archive hashes, binary hashes, before/after reports and both-host logs are under `/Users/davafons/.local/state/parcela/agent-tests/lifecycle-retry-20261013T010300Z`, with Tower artifacts under the same private suffix. Every recovery fixture verified service listener closure. The startup and mixed command unit below covers qualified pre-readiness recovery and refusal of missing allocation evidence. Concurrent up and restart, successor cleanup, immutable launch snapshots, reboot recovery, and active handoff remain unverified. Independent review remains open. No binaries were installed or primary services changed.

### Pre-readiness recovery and mixed lifecycle callers

Current `scripts/check` passes on macOS and Tower running Linux with 71 Rust tests and 20 Python tests. Both hosts pass normal live acceptance and all 20 real backend recovery cases. The added startup cases recover a qualified native graph before HTTP readiness without replaying the service or publishing its route early. A qualified stop also completes before readiness. An allocation that lacks durable launch evidence refuses `down` and repeated `up`, retaining its lease and source data. That result proves refusal, not automatic allocation recovery.

The mixed `recover` and `down` case failed on both hosts before the fix. The stop caller waited for the owner lock while the recovered controller monitored its surviving graph indefinitely. A focused Rust regression confirmed that stop intent never reached the new controller. The caller now forwards stop through the selected generation's live control socket while waiting. Controller birth, socket inode, and held original lock evidence must match. The durable stopping phase records admission without a new persisted flag. The timeout diagnostic reports incomplete ownership rather than promising no signaling after forwarded stop intent.

The real mixed race now verifies two successful exits, exactly one native shutdown request, and no service replay. Rust guards also verify that a replaced control socket receives no stop request and that successor generations remain protected. The failing tests are in `xkkqotrr/8b14f97c`. The fix is in child `mrlvnkyp`. Source and binary hashes, before and after reports, and both-host logs are under `/Users/davafons/.local/state/parcela/agent-tests/lifecycle-startup-20261013T013518Z`. Tower artifacts use the same private suffix. Every recovery fixture confirmed service listener closure.

Automatic reconciliation without launch evidence remains unsupported. Concurrent startup and restart, immutable launch snapshots, stale successor cleanup, qualified reboot recovery, and active environment handoff remain unverified. Independent review remains open. Installed binaries, primary services, Yuke state, trust, and exposure were unchanged.

## Real-machine cutover state

User-authorized user-local binaries and the unprivileged `org.parcela.proxy` launch agent are installed. The real Shirabe checkout is adopted as primary; its launcher/manifest use Parcela and Process Compose, without the rb-portless gem/binstub/Procfile. Its existing development databases and uploads remain in `storage`; no preparation/reset/import was run.

The user retired the shared rb-portless front door, accepting loss of Nadeshiko routing, and authorized PF forwarding, the boot job and System-keychain CA trust. The operator setup helper completed successfully. `parcela proxy status` verifies the owned admin endpoint and standard-port forwarding. After the user started `bin/dev`, `/usr/bin/curl -q --noproxy '*' -D - https://shirabe.localhost/up` returns HTTP/2 200 with system trust, without a custom CA or hostname override. HTTP port 80 redirects the same path to HTTPS with status 308. Process Compose's `process list --output json` over the environment's private `pc.sock` reports web, CSS, jobs and SDK as Running. This proves startup and the health route, not full application or watcher acceptance.

`launchctl print system/org.parcela.pf` shows the forwarding job loaded with last exit code 0. The installed privileged helper matches `scripts/setup_macos.py`. Reboot persistence and browser trust remain unverified. No primary application services, jobs, migrations or imports were launched during this host setup.

## Gaps

No automatic crash recovery or takeover of legacy owners, interactive attach, service-specific restart, following logs, alias/tenant routing, sharing, general data-isolation certification, automatic backend packaging or source/data destruction. Daemonized descendants and failed route revocation are not magically reconciled; uncertain leases remain recorded. HTTP readiness is not continuous after listener establishment; status probes it on demand.

Actual Shirabe Cable transport, linked SDK rebuild behavior, full application/searchable-corpus regression, broader application HTTP/2 behavior, active native agent handoff, a Mac-to-Linux-to-Mac round trip, release packaging, browser trust and reboot persistence remain unverified. The privileged PF/trust helper succeeded on this host; custom/conflicting PF ruleset handling has only nonprivileged guard checks. The PRD's P0 goals are not a shipped-feature list.
