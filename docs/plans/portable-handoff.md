---
owner: portable-handoff workspace
status: in_progress
resume: acceptance-05 passed paused full-data Shirabe acceptance on Tower; its report has complete true, health/search 200, preserved queue/snapshots/uploads, and route teardown 503. Code checks pass on macOS/Linux with 54 Rust and 20 Python tests. Next reconcile Yuke's stale workspace while preserving on-disk changes, then finish native profile/context portability and source fencing before Herdr or round-trip acceptance. No active agent execution or ownership transfer has occurred.
---

# Portable handoff execution plan

[Portable environments](../portable-environments.md) owns the selected contract. [Transfer commands](../transfer.md) documents implemented primitives, not complete handoff. [Check a paused Shirabe replica](../check-portable-shirabe.md) owns the destination acceptance procedure.

## Authority and ownership

The user authorized continuing toward full environment handoff and actual Shirabe/Tower acceptance. Start isolated replicas before touching live writers. Credentials remain destination-local. No system binary replacement, privilege/trust changes, implicit exposure, GUI focus/input, clipboard changes, or unrelated cleanup. The Mac has insufficient free space for another full dictionary copy; preserve direct-to-Tower streaming.

Own jj workspaces:

- Parcela: `/Users/davafons/workspace/parcela-portable-handoff`. `mkrxrxqs` contains the inherited transfer foundations; `sxlwquyo` contains destination acceptance. Both are stacked on current `main`. The default workspace remains unchanged on main.
- Yuke: `/Users/davafons/workspace/yuke-portable-handoff`, `pvnsrzuy`. Its working copy is stale. Inspect and preserve its on-disk changes before `jj workspace update-stale`; do not restore an older revision over them.
- Shirabe: `/Users/davafons/workspace/shirabe-portable-handoff`, `luzvktns`. Own transfer policy and upload helper/tests. Do not alter the user's primary checkout or live data.
- Herdr: no integration changes yet. Do not implicitly transfer the mixed `multidev` workspace.

## Milestones

- [x] Strict policy and metadata-only read-only transfer preflight; full readiness stays false.
- [x] Pinned jj/history checkpoint, independent linked-workspace restore, private SHA-256 payloads, atomic no-replace publishing, explicit source-only scope.
- [x] Local SQLite backup and quiescent files; task/profile capture remains unsupported and rejected.
- [x] Streamed SQLite backup via SSH without full local staging; bounded buffers/deadline, source low-space guard, validation and no-replace publication.
- [x] Native idle-tree Yuke checkpoint prototype and imported-tree fence; inspection does not consume queues.
- [x] Refreshed source checkpoint and snapshot-referenced upload capture transferred to Tower.
- [x] Destination Ruby dependencies installed in task-local bundle paths with the frozen lockfile unchanged. Pinned Caddy/Process Compose archives and lsof source verified against upstream release SHA-256 and installed only under the task root.
- [x] Complete paused full-data Linux service acceptance using the final repeatable check (`acceptance-05`).
- [ ] Native profile portability, extension writer barrier, destination context reconciliation, and active source quiescence.
- [ ] Complete checkpoint coordinator, resumable upload, divergence/repeat-import handling, cross-host ownership generations, activation receipts, and failure recovery.
- [ ] Herdr selection/layout/agent associations and a stable entry point.
- [ ] Integrate Shirabe policy/upload tasks and safe copied-queue behavior into the primary project after verification.
- [ ] Actual full-data services/layout/native continuing-thread Mac -> Tower -> Mac acceptance, repeated transfer and failure injection.
- [ ] Native Codex/Claude transfer, separately verified. Tower credential use is not Codex thread transfer.

## Checks

Current Parcela `scripts/check` passes 54 Rust tests, 20 Python tests, formatting, Clippy and schema validation on macOS and Tower/Linux. Python coverage includes immutable inspection, strict retained-input WAL refusal, stopped-queue fingerprints, row/schema/application metadata changes, live sidecar refusal, opener-result safety, private paths, disabled effect-producing services, explicit test ports, receipts, interrupt handling, output/input separation, state capacity, socket-path limits, tokenizer binding/edition safety, ASCII-safe Rails runner code and durable failed-command diagnostics. The stopped-queue regression failed with `Snapshot must be normalized to rollback-journal mode` before the fix, then passed. The tokenizer-binding regression previously failed with `KeyError: KABOSU_DICT_DIR`. ShellCheck passes. Local Markdown path checks pass for 15 links across the six relevant documents.

Tower's generic live backend acceptance passes with actual Caddy/Process Compose/lsof: simultaneous HTTPS routes, forwarded-header spoof rejection, SSE/WebSocket, unknown-host/duplicate-owner/foreign-port refusal, signal/exit handling, native web+worker UDS graph and independent teardown. HTTP 34839 / HTTPS 56943 were explicit disposable test ports. This does not establish Shirabe full-data acceptance or standard-port setup.

Shirabe's upload helper passes its 3 tests locally. The earlier `bin/house-rules` failure came from Git-dependent tooling in a linked jj workspace. No Ruby-visible application code changed in this session. The live primary source/data remain untouched.

Inherited Yuke evidence: seeded native Zig tests and 9 Python tests passed on macOS/Tower. Earlier export verification created SQLite sidecars; the on-disk implementation fixed snapshot normalization and immutable verification. These claims need reconciliation with the stale working copy before editing or describing that change.

## Artifacts and resume point

Tower task root: `/home/davafons/.local/state/parcela/agent-tests/portable-handoff`.

The old stream job's local log and transport receipt are unavailable. Tower retains all four published snapshots at `runs/0314532e14c8491683143d8ea597d891`: primary 871,796,736 bytes, dictionary 17,214,709,760 bytes, queue 34,615,296 bytes, cache 58,933,248 bytes. No transfer processes were running at pickup. acceptance-01 rehashed and validated all four snapshots successfully; its private receipt establishes a new baseline. Do not claim to have recovered the original transport receipt. These snapshots are independently consistent, not an atomic application checkpoint.

Current task directory: `resume-20261012` under the Tower root. Local artifacts: `/Users/davafons/.local/state/parcela/agent-tests/portable-handoff-resume-20261012`.

- `shirabe-code-checkpoint`: source checkpoint `1e786922-fd61-4cc0-a45d-316a59d0e485`, revision `3243a1b425ba60b10bb50f899f2a0f03248c997c`, includes the upload helper/tests. It replaces the old source-only artifact for acceptance, without overwriting it.
- `uploads-snapshot`: 414 local blobs, 276,803,380 bytes, selected from Tower's captured primary database. Source and captured files matched the database's Active Storage size/MD5 metadata. The snapshot is transferred; the acceptance script also compares captured/restored SHA-256 values.
- `build-replica`: independent source restore used for frozen bundle installation, not service activation. Lockfile SHA-256 before/after matches.
- `gems`, `bundle-config`, `bundle-home`: task-local Ruby dependency paths. Ruby 4.0.2 already exists at `/home/davafons/.local/share/mise/installs/ruby/4.0.2/bin`; use that explicit PATH, not the host's Ruby 3.4 or an unconfigured mise bundle shim.
- `tools`: Caddy 2.11.7, Process Compose 1.122.0, lsof 4.99.5. `tools-checksums.txt` records verified archives. No system install or trust action occurred.
- `parcela-check-src`: latest copied Parcela source for Linux checks. Existing Rust 1.95.0 is selected through `RUSTUP_TOOLCHAIN`; the task's old mise Rust entry points at the existing rustup binaries.
- `acceptance-01`: failed before services started because copying the dictionary into `/tmp` exceeded its quota. All input snapshots validated first. Its private `report.json` and `failed-state` archive preserve diagnosis. The task-owned tmpfs state was moved into that disk-backed archive; original snapshots were not moved or modified.
- `acceptance-02`: disk-backed replica passed named HTTPS `/up`, actual Rails corpus access (3,659,561 entries), Active Storage sample download and 414 upload records, with web/CSS Running and test queue/mail adapters. Search returned 500 because no tokenizer dictionary existed in the private home. Both foreground children stopped cleanly. Preserve its failed report/logs.
- `tokenizer`: private Sudachi full edition `20260723`, selected by Shirabe's own tasks/health check. Upstream wheel SHA-256 `c0abc12bb4aaab8042a8fb8cf4d6ce0dd78b97f099cecf932665e22688ab5ed9`; installed dictionary SHA-256 `8f57318aa7e983da7eff1a9c10b733221e6428622faaee78526e0e33f30734de`, 359,685,569 bytes. `tokenizer-install.json` records the receipt; the upstream license is retained.
- `debug_search.py` and `debug-search.*`: focused in-process Rails request against the existing paused replica. With only the tokenizer binding added, search returns 200 and tokenization returns `猫`. Its temporary manifest changes were restored. This proves the dependency fix, not full fresh-run acceptance.
- `acceptance-03`: failed in the Rails probe under Tower's non-UTF-8 locale. The inline Ruby query now uses an ASCII-safe Unicode escape. A focused probe passed under `LANG=C LC_ALL=C`. Failed-command output is now retained in a private receipt.
- `acceptance-04`: health/search returned 200, Rails read the full corpus and captured upload, and route teardown returned 503. Final queue validation incorrectly required rollback-journal format for the stopped runtime database. Read-only comparison found exactly four changed bytes at offsets 18, 19, 27 and 95; all bytes after the header and both SQL dumps matched. No openers or sidecars remained.
- `acceptance-05`: passed with the queue fix, disk-backed `--state-parent /home/davafons/.parcela-tests`, explicit tokenizer, `LANG=C LC_ALL=C`, and loopback test HTTP 18180 / HTTPS 18443. `report.json` has `complete: true`, health/search 200, preserved queue/snapshots/uploads, and route teardown 503. Foreground app/proxy exited 143/0. State is `/home/davafons/.parcela-tests/ph-voq6pqxz`; preserve it and the completed run.

The canceled local SSH call did not stop Tower's acceptance process. Observer `job-nbzr` waited for its exit and returned 0; no acceptance job remains pending. Retained inputs still require finalized rollback-journal snapshots and exact SHA-256 preservation. Runtime queue fingerprints permit only SQLite journal-mode/counter header changes after verified shutdown; all other bytes must match. See [transfer commands](../transfer.md#destination-shirabe-acceptance).

## Inherited native proof

A real idle Shirabe thread `01a124e68db37b2c8975d2fc6c85434f` (187 messages) was imported fenced into Tower `native-proof/data/yuke`. Linux RPC loaded its history with a rebound execution root; queues and high-water marks stayed unchanged. Instructions/skills differed and reconciliation was not finished. This was idle-thread inspection, not resumed execution.

Tower `yuke-idle-checkpoint-v2` is the valid inherited native artifact. Preserve `yuke-idle-checkpoint` as a failed artifact with verification-created WAL/SHM files. Do not reuse it.

A separate synthetic Tower session completed a real model turn using destination-local authentication. The imported old thread stayed fenced. That historical test does not establish that the temporary credential remains valid now.

No live source execution fence, current-thread handoff, Herdr layout restore, or complete round trip has happened. The earlier live thread `01a1266e3bbb7c6e90f301f5b33e5eeb` has not been touched; do not assume it is now idle.

## Next after paused acceptance

Reconcile Yuke's stale workspace, then implement profile/context reconciliation and durable source fencing/target activation. Keep the imported native thread fenced until those checks pass. Validate copied queue policy before enabling any effect-producing worker.

The 16 GiB return checkpoint cannot be blindly duplicated on the Mac. Measure task-owned APFS cloning and delta transfer against an immutable Tower artifact, with a final SHA-256 and no writes to the primary source. Then add Herdr selection/layout and prove the complete round trip with interruption, lost acknowledgment, repeated activation, and divergence cases.
