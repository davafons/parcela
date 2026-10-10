---
owner: portable-handoff workspace
status: in_progress
resume: Full Shirabe DB stream job-x486 is running; await its completion, do not poll. Finish project upload capture/tests, native context/profile reconciliation and fencing controller, then Linux services/Herdr/round-trip acceptance. Source-only code and one real idle native thread are restored on Tower; current live thread remains untouched.
---

# Portable handoff execution plan

[Portable environments](../portable-environments.md) owns the selected contract. [Transfer commands](../transfer.md) documents implemented primitives, not complete handoff.

## Authority and ownership

The user authorized actual Shirabe/Tower acceptance and a repeatable Herdr entry point. Start isolated replicas before touching live writers. Credentials remain destination-local. No system binary replacement, privilege/trust changes, implicit exposure, GUI focus/input, clipboard changes, or unrelated cleanup. On 2026-10-10 the user selected direct-to-Tower streaming because the Mac has 7.6 GiB free versus a 16 GiB dictionary.

Own jj workspaces:

- Parcela: `/Users/davafons/parcela-portable-handoff`, `mkrxrxqs`, parent runtime `nrlxzzuw` is another session's work. Inspect state before stale updates/history changes; preserve its edits.
- Yuke: `/Users/davafons/workspace/yuke-portable-handoff`, `pvnsrzuy`, based on the existing current-code stack. Native fence admission and Python checkpoint/inspection tools only.
- Shirabe: `/Users/davafons/workspace/shirabe-portable-handoff`, `luzvktns`, source version inherited from the active checkout. Own transfer policy and upload helper/tests. The user's primary checkout is unchanged by these edits.
- Herdr: no integration changes yet. Do not implicitly transfer the mixed `multidev` workspace.

## Playbook

1. Read the plan that owns the work first. A stale plan never outranks the code.
2. One agent owns one plan file. Keep its resume state current; overwrite, do not append.
3. Promote settled facts to the permanent doc, then delete the plan.

Feature work:

1. Simplest thing that works. Not the general case, not the speculative one.
2. Put behaviour at its owning boundary. Follow the project's architecture.
3. Test the behaviour, not the implementation.
4. Changelog note.

## Milestones

- [x] Strict policy and metadata-only read-only transfer preflight; full readiness stays false.
- [x] Pinned jj/history checkpoint, independent linked-workspace restore, private SHA-256 payloads, atomic no-replace publishing, explicit source-only scope.
- [x] Local SQLite backup and quiescent files; task/profile capture remains unsupported and rejected.
- [x] Streamed SQLite backup via an unpublished destination VFS over SSH, bounded buffers/pipe deadline, source low-space guard, native validation and no-replace publication. Local and Linux tests pass.
- [x] Native idle-tree yuke checkpoint prototype, selected rows/blobs/inbox, structured path rebinding, stale prompts, native imported-tree fence. Native inspection does not consume queues.
- [ ] Native profile portability, extension writer barrier, destination context reconciliation, and active source quiescence.
- [ ] Complete checkpoint coordinator, resumable upload, divergence/repeat-import handling, cross-host ownership generations/activation receipts/failure recovery.
- [ ] Herdr selection/layout/agent associations and a stable entry point.
- [ ] Shirabe owned policy/upload tasks, safe copied-queue behavior, Linux dependencies and restart recipes. Policy and upload helper drafted/tested; not yet integrated into the primary checkout.
- [ ] Actual full-data services/layout/native continuing-thread Mac -> Tower -> Mac acceptance, repeated transfer and failure injection.
- [ ] Native Codex/Claude transfer, separately verified. Tower's Codex credential use is not Codex thread transfer.

## Verified state

Parcela `scripts/check` passes 54 tests, formatting, Clippy and generic validation. Eleven checkpoint tests include linked jj independence, WAL state, source-only omissions, and concurrent empty-destination preservation. Four streaming tests pass on macOS and Tower/Linux, including concurrent source writes, FTS5, NUL text, blobs, failed bounds/integrity and interruption. Linux all-target Clippy passed before the latest source-only changes; rerun full checks on the final code.

Yuke `zig build test --seed 0` exited 0; the fence test proves queued input stays untouched and unrelated roots can still be owned. Nine Python tests pass on macOS and Tower. A real WAL-state export exposed verification-created sidecars; backup now normalizes its private snapshot to DELETE mode, immutable verification creates no sidecars, and a regression test verifies twice. Run unseeded native tests before describing.

Shirabe upload helper tests pass 3 cases: referenced-file round trip, missing/corrupt/linked/external-service refusal, traversal/ancestor-link/no-overwrite checks. `config check` passes. `bin/house-rules` failed in the linked jj workspace because it invokes Git without a `.git` directory; use a verified read-only repository context or an independent colocated snapshot, not mutating Git. No Ruby-visible application code changed yet.

## Actual Tower artifacts

Task root: `/home/davafons/.local/state/parcela/agent-tests/portable-handoff`.

Task-local Rust 1.95.0 and Zig 0.16.0 are installed under `tools`, not on the normal PATH. Linux Parcela release and yuke ReleaseFast binaries are built at `parcela-src/target/release/parcela` and `yuke-src/zig-out/bin/yuke`. Caddy, Process Compose and lsof are not available on the normal SSH PATH yet.

Source-only Shirabe checkpoint restored independently to `shirabe-replica/source`, commit `4ce47bd5c177484dc9e2a373ad84230905ca5de9`, checkpoint `6d1d9615-55bf-46e2-b032-0e3fcd0d7880`. Source artifact: `/var/folders/b0/ggkwwpj56nx_kntnkyr5w3p80000gn/T/parcela-shirabe-source-6n28_ijn/checkpoint`. This predates upload-helper tests/docs and must be recaptured for final acceptance.

A real idle Shirabe thread `01a124e68db37b2c8975d2fc6c85434f` (187 messages) was exported with inbox state and imported fenced into `native-proof/data/yuke`; source checkpoint: `/var/folders/b0/ggkwwpj56nx_kntnkyr5w3p80000gn/T/yuke-shirabe-native-proof-edol6elo/checkpoint`, Tower `yuke-idle-checkpoint-v2`. Native Linux RPC loaded the exact session/history with its execution root rebound to the Shirabe replica. Registry/high-water marks and queued inputs were unchanged. Instructions/skills differ and reconciliation remains required. This is an idle-thread import, not handoff of the running thread.

An earlier checkpoint at `yuke-idle-checkpoint` acquired unlisted WAL/SHM files during old verification and was refused; preserve it as a failed artifact. Do not reuse it.

Tower's existing `~/.codex/auth.json` contains a current OAuth access token. A private task-local yuke provider file uses that destination-local token without copying a refresh token. No Mac credential was transferred; the existing Codex auth file's bytes remained unchanged. A separate new synthetic native yuke session `01a126db58f37d7b944ad2f20e869b0e` completed one real model turn with `TOWER_AUTH_OK`. The imported old thread stayed fenced. This proves live target authentication, not transferred-thread continuation. The task credential expires and is not a permanent login setup.

## Running work and next actions

`job-x486` runs `scripts/check_sqlite_stream.py` for the actual primary/dictionary/queue/cache files. Log: `/var/folders/b0/ggkwwpj56nx_kntnkyr5w3p80000gn/T/yuke-287-71f0df66/job-73.log`. Await completion; do not poll. Its completion log names the source receipt directory and fresh Tower `runs/<uuid>` directory. Snapshots remain paused and independently consistent; live primary writers were not fenced. Queue jobs must not start during validation.

After completion, use the captured primary snapshot's Active Storage references to capture the corresponding uploads. Observed live metadata has 414 local blobs, 276,803,380 bytes; do not use that moving count as the immutable snapshot's expected inventory. Verify each file against its captured MD5/size and retain SHA-256 transfer receipts.

Build/install Caddy, Process Compose and lsof only in the task tool directory with pinned upstream artifacts; verify Linux service readiness with explicit loopback test ports and disposable CA, not system setup. Install Ruby gems into task-local paths with frozen lockfile. Source-side port forwarding, if needed, must use explicit nonconflicting loopback endpoints.

The 16 GiB return checkpoint cannot be blindly duplicated on the Mac. Investigate task-owned APFS cloning plus rsync delta updates against the immutable Tower artifact, with full final SHA-256 and no writes to the primary source. Do not claim that reverse full-data transfer works until measured.

The current live thread `01a1266e3bbb7c6e90f301f5b33e5eeb` is still running and must not be exported as idle. No source execution fence, current-thread handoff, Herdr layout restore, service migration, or complete round trip has happened.
