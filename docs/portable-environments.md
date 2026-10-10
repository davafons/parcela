# Portable environment handoff

## Goal and selected scope

Copy a consistent environment checkpoint to another host, validate it there, and continue the work on the destination. Retain the source as a recoverable checkpoint. The initial supported host pair is macOS and Linux over SSH; other hosts require their own acceptance tests.

The user selected these directions on 2026-10-10:

- Handoff active work, rather than continuous two-way synchronization. Only one replica actively continues the same logical work.
- Include declared application data and portable agent profiles. Credentials remain destination-local.
- Prove yuke first, then add native Codex and Claude transfer.

This is a goal, not a claim that handoff works today. The implemented slices are read-only preflight, paused jj/resource checkpoints, and streamed SQLite capture. Their contract is in [transfer commands](transfer.md). Execution ownership, agent activation, and Herdr handoff remain incomplete; [the execution plan](plans/portable-handoff.md) records the current work and evidence.

## Ownership

Parcela coordinates checkpoint identity, declared resources, transfer, restore, and handoff. Project configuration owns data policy and preparation commands. Process Compose remains the service supervisor; Caddy remains the routing backend.

Yuke owns native export/import of its sessions and plugin-owned state. Codex and Claude integrations must establish version-aware native persistence and resume contracts. Herdr supplies workspace selection, layout, pane associations, and launching resumed agents. Parcela does not become an agent engine, a terminal emulator, or another supervisor.

The unit of transfer may include multiple checkouts. A Herdr workspace is presentation/session organization, not a guarantee that all panes use one project. A pane sharing writable resources with other agents cannot be independently handed off without either including those writers or making an explicit independent copy.

## State boundary

Copy the exact selected source state, needed VCS history, native thread history and supported relationships, referenced attachments, declared application data, and portable configuration. Account for untracked/ignored source files explicitly. Linked jj workspaces and Git worktrees must restore without pointers to repositories left on the source host.

Reconstruct dependencies, services, terminals, and endpoints on the destination. Native compiled dependencies and generated caches are not cross-platform artifacts. Report missing executables, unsupported versions, undeclared resources, and host-specific configuration before activation.

Do not transfer process IDs, sockets, kernel locks, live PTYs, machine credentials, signing keys, or authorization grants. A macOS process is not migrated into a Linux process. Live model requests and ambiguous interrupted commands must terminate or be recorded as interrupted, not replayed as if they had never started.

Preserve native agent context where supported. A summary-based new conversation is an explicitly requested fallback, not successful native transfer. Record limitations for child threads, compactions, pending inputs, plugin state, and external resources instead of silently omitting them.

## Checkpoints and identity

Separate logical environment identity, per-host replica identity, immutable checkpoint identity, and native agent IDs. Checkpoints name their parent, captured resource hashes, source versions, path mappings, exclusions, and unresolved requirements. An imported replica starts paused.

Subsequent handoffs compare both replicas with their common checkpoint. An unchanged replica can accept the selected update. Diverged source code uses an explicit VCS resolution workflow; application databases and agent histories are not generically merged or overwritten. Preserve both sides and require resolution or a separate copy.

Rebind structured execution paths to destination roots. Preserve historical transcript text. Reconcile current instructions, skills, hooks, and permission settings before future execution; source-host grants must not become destination authorization. Do not transplant stale prompt paths or silently broaden permissions.

## Consistency and activation

Quiesce all included writers while capturing code, threads, and data. An uncooperative external editor or process is a consistency gap, not something a local before/after fingerprint can certify. SQLite capture uses a consistent backup or verified quiescence, including correct WAL handling. Project commands own other snapshot/restore policies.

Restore and validate with target agents and effect-producing workers paused. Readiness checks must not consume queued messages or execute application jobs. Pending agent work is retained for explicit continuation. Pending application jobs need a project-specific external-effects policy; copying a queue is not permission to resend email or repeat payments.

Before target activation, obtain a durable source fence bound to the checkpoint and ownership generation. While fenced, the source retains files/data but cannot admit new managed agent or service work. Target activation and its acknowledgment are recorded durably and are idempotent.

A lost acknowledgment or unreachable host is unknown ownership, not permission to start the same work on the other host. Reconcile before retry or rollback. After a failure before activation, recover the source only when the destination is known not to be executing. After successful handoff, an explicit later handoff is required to continue on the source.

These are operational safeguards among cooperating tools, not containment against someone manually starting same-user processes or disabling a fence.

## Privacy and host safety

Use authenticated SSH transport, private staging directories, payload integrity checks, and atomic import. Treat thread/tool output and application data as sensitive even when explicit credentials are excluded. Known credential filename exclusions cannot certify arbitrary content as secret-free.

Profiles use declared include paths and explicit host-local roots. Do not copy a whole home directory. Credentials are provisioned separately; destination readiness reports missing requirements without retrieving or printing credential values.

Installation, privileged setup, CA trust, network sharing, and moving live user work have distinct authorization boundaries. Do not replace system tools, take over listeners, expose services, or delete source/data during an ordinary transfer. Cleanup targets only identified transfer-owned artifacts.

Remote URLs are separate from source-host `.localhost` URLs. A destination service is not browser-accessible from the source machine merely because it passed a loopback readiness probe.

## Acceptance

Before advertising handoff, prove a real Mac-to-Tower-to-Mac round trip with exact source/VCS state, native yuke parent/child state, attachments, compactions, pending questions, declared SQLite/uploads, portable profile configuration, and reconstructed services. Show that unrelated destination work and the retained source remain intact.

Repeat import and activation without duplicated sessions or repeated queued work. Inject failures during capture, upload, restore, fencing, activation, and acknowledgment. Detect divergent replicas and reject unsupported required state. Test native Codex and Claude separately before claiming their compatibility.
