# Foundation configuration and CLI

This is the executable Rust **foundation** contract, not the complete v1 runtime
schema. New lifecycle/resource fields will require explicit schema evolution.
Unsupported organizer fields fail instead of silently implying isolation.

## One maintained manifest

Commands find the nearest `parcela.yaml` at or above `--path` (current directory
by default). Its directory is the checkout root. Keep the same project ID/name
in each checkout; environment labels/roles belong to the machine registry.

```yaml
version: 1
project:
  id: shirabe             # stable across all its checkouts
  name: shirabe           # DNS-safe, separate from ID
default_service: web
services:
  web: {}                 # environment base hostname
  api: {}                 # api.<environment-base>
  metrics:
    subdomain: stats      # explicit stats.<environment-base>
tasks:
  test:
    argv: [bin/test]      # literal argv, not shell text
    timeout_seconds: 300
```

Project IDs contain 1–128 ASCII letters, digits, hyphens, underscores or dots.
Project names, service keys, subdomains, task keys and adoption labels contain
1–63 lowercase ASCII letters/digits with internal hyphens. Invalid names are
rejected, not silently sanitized. Preview labels joined to project names must
also fit one 63-byte DNS label. The default service must exist and cannot have a
subdomain; duplicate additional-service subdomains fail.

Tasks require a nonempty executable, no NUL in argv and a positive timeout when
provided. They are **validated but not executed**. Preparation/data policy will
remain project-owned; no fresh/clone database policy is inferred from this file.

An optional `process_compose` mapping stores native graph data, preserving native
fields rather than parsing them as organizer fields. Unknown keys outside that
mapping fail. See [the generic example](../examples/generic/parcela.yaml).
`config check` validates the organizer schema and mapping shape only; it does
**not** validate the graph against Process Compose or prove that its commands
consume their endpoints. No graph/context is generated for Process Compose yet.
Parcela's environment/resource binding and standalone-command contracts remain
future runtime work. Do not run the example graph expecting isolation.

## Implemented commands

All successful commands print JSON; diagnostics go to stderr. Operational errors
exit 1, argument/unsupported-command errors exit 2 (Clap). Distinct lifecycle,
conflict and timeout exit categories are not implemented yet.

| Command | Behavior |
|---|---|
| `config check` | Parse/validate the nearest manifest without creating state |
| `workspace adopt --role primary\|preview --label LABEL` | Register canonical checkout path and immutable UUID; role/label required |
| `workspace list` | Inventory scoped to current manifest project |
| `workspace list --all` | Explicit inventory across the selected registry; no manifest needed |
| `workspace inspect` / `context` | Resolve current adopted checkout; planned service URLs, capability caveats |
| `doctor` | Static implementation/backend-boundary report; no tool/version/health probes |

`--path` and `--state-dir` are global options. No current command prompts or
starts a child, edits VCS history, opens a browser, installs software, trusts a
CA, changes a listener, or deletes source/data. The CLI deliberately does not
accept unimplemented `run`, `up`, `stop`, `task`, `setup` or sharing commands.

## Registry and identity

Only adoption creates `<state>/registry.sqlite3`. New state directories use mode
700 and the database uses mode 600. Existing state/database must be user-owned
and private; final state-directory/database symlinks are rejected. This is an
operational ownership boundary, **not hostile same-user code containment**.
Inspection opens the database read-only and returns empty inventory for absent
state. It does not automatically repair or migrate malformed/future state.

Immediate SQLite transactions serialize adoption; durable constraints cover
project names, canonical source paths, per-project labels, one primary per
project, and environment base hostnames across projects. A conflicting
transaction rolls back rather than changing another owner's record. Branches,
bookmarks and revisions are never identity inputs. jj metadata is detected
before Git metadata in colocated roots; no VCS subprocesses are invoked. VCS
kind is an adoption-time observation, not current revision metadata or a
verified discovery implementation.

Repeated adoption of the same canonical source/project/role/label returns the
same ID. Symlink aliases of a source resolve to the same path. Changed manifest
identity/name or adoption role/label fails; explicit rename/rebind is not yet
implemented. Missing sources remain in inventory with `source_exists: false`;
no garbage collection or deletion is inferred. Each inventory entry contains a
`workspace` record and `source_exists` observation; `runtime_verified` is false.
Non-UTF-8 checkout paths are not supported for adoption yet.

Context includes `state: registered`, `generation: 1`, `planned_urls`, a reserved
`runtime_dir` path and `runtime_created: false`. Neither generation nor path
means resources/endpoints have been allocated. Routing, supervision, tasks,
sharing and verified data isolation are false. No process/socket/port leases,
secrets, data baselines, runtime directories or trust records exist yet.
