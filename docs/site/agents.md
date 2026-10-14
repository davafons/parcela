# Work with a coding agent

## Load the operating instructions

Use the same Parcela binary that the agent will operate:

```sh
parcela --skill
```

`parcela skill` prints the same bundled Markdown. Neither command reads the project or registry, starts a backend or changes a profile. It works outside a configured checkout. It cannot be combined with a lifecycle command.

An agent can load the returned text into its context. The [operating skill](../../.agents/skills/parcela/SKILL.md) is also readable here. [Download its Markdown](../../.agents/skills/parcela/SKILL.md?raw=1) and save it as `SKILL.md` if your agent runner supports skill directories. Parcela does not install a global skill automatically.

## Give the agent an explicit scope

State the intended checkout or environment ID, registry when nondefault, and operation. Describe any queue, worker or data effects it may perform. For example:

> Inspect this preview's status and logs. Do not restart it, run preparation tasks or change the HTTPS front door.

Or:

> Start this configured preview detached after checking its private data bindings and dependency readiness. Verify the frontend and API paths. Leave it running.

Reading the skill grants no additional authority. Project instructions and the user's requested scope still apply.

## Start with inspection

```sh
parcela workspace list --all
parcela --path "$CHECKOUT" context
parcela --path "$CHECKOUT" status
```

Inventory returns registered IDs and observations. Context returns planned metadata, not proof of health. Read `parcela.yaml` before acting. Use the same binary and state directory for later commands.

For a known UUID:

```sh
parcela --workspace "$ENVIRONMENT_ID" status
parcela --workspace "$ENVIRONMENT_ID" services
parcela --workspace "$ENVIRONMENT_ID" logs backend --tail 100
```

Select an actual native process name for logs. Inspect `state`, `live_health_verified` and `live.endpoints` together. The verification flag means an owner replied; it does not mean every HTTP service passed its probe.

## Keep mutations separate

After explicit authorization and prerequisite checks:

```sh
parcela --workspace "$ENVIRONMENT_ID" up --detach
parcela --workspace "$ENVIRONMENT_ID" status
parcela --workspace "$ENVIRONMENT_ID" url --service backend
```

Start only the selected environment. Do not infer permission for migrations, reset, seed, import, code generation, Docker startup, CA trust or listener takeover. Do not borrow another agent's database or storage service.

A task needs its own effect review. Recovery needs qualified ownership. Stop is not automatic cleanup of a user-managed environment just because an agent's test ended.

## Return evidence, not intent

Ask the agent to report the selected environment ID, role, generation, actual URLs, fresh health and checked application paths. Require it to state what it did not verify.

When ownership is unknown or recovery refuses, preserve diagnostics and ask for an intervention plan. A saved PID never authorizes signaling. Do not use broad process kills, delete control sockets or remove leases to make a command appear successful.

Herdr can organize the agent's terminals. Parcela owns the development graph. A separate terminal pane for each service is not required.
