# Work with a coding agent

Use your existing agent session to configure and operate a Parcela environment. The agent uses its current model, tools, billing and permissions. You do not need a separate AI setup service or a Parcela model API key.

## TL;DR: ask your agent to configure this checkout

If Parcela is already installed, paste this request into your coding agent:

```text
Configure only this checkout for Parcela.

Read the project instructions and the output of `parcela --skill`.
Read https://parcela.davafons.com/docs/services/ for the configuration contract.
Inspect existing development and test entry points.
Do not read .env files, credentials or native control tokens.

Propose a service graph, readiness checks and primary/preview data bindings.
Use the project's actual commands. Do not assume a framework or database layout.
If database, storage or queue policy is unclear, ask before proceeding.

Add or minimally update parcela.yaml. Preserve existing commands and configuration.
Propose application-code or .env changes separately. Do not apply them without approval.
Run `parcela config check` and report missing dependencies or unsupported behavior.

Do not install tools, adopt a workspace, start services, run migrations, seed/reset data,
change CA trust or forwarding, or edit global agent settings without separate approval.

Return the configuration diff and exact adoption, startup, health-check and test commands.
Distinguish completed validation from checks that still require approval or prerequisites.
```

This request authorizes project configuration, not host setup or application preparation. `config check` validates organizer fields; it does not prove native graph compatibility, data isolation or application health.

If the binary is missing or lacks `--skill`, report that first and follow [build and host setup](install.md). The [downloadable operating instructions](../../.agents/skills/parcela/SKILL.md?raw=1) are also available without source-repository access. Reading them does not install the CLI.

## Load the operating instructions

Use the same Parcela binary that the agent will operate:

```sh
parcela --skill
```

`parcela skill` prints the same bundled Markdown. Neither command reads the project or registry, starts a backend or changes a profile. It works outside a configured checkout. It cannot be combined with a lifecycle command.

An agent can load the returned text into its context. The [operating skill](../../.agents/skills/parcela/SKILL.md) is also readable here. [Download its Markdown](../../.agents/skills/parcela/SKILL.md?raw=1) and save it as `SKILL.md` if your agent runner supports skill directories. Parcela does not install a global skill automatically.

## Install the skill for Claude Code

[Claude Code discovers skills](https://code.claude.com/docs/en/skills) from `.claude/skills/<name>/SKILL.md` in a project. Run this block from the project root, using the Parcela binary that will operate its environments:

```sh
(
	set -eu
	skill_dir=.claude/skills/parcela
	mkdir -p "$skill_dir"
	stage=$(mktemp -d "$skill_dir/.install.XXXXXX")
	trap 'rm -f "$stage/SKILL.md"; rmdir "$stage"' EXIT
	parcela --skill > "$stage/SKILL.md"
	test -s "$stage/SKILL.md"
	ln "$stage/SKILL.md" "$skill_dir/"
)
```

The block installs the complete bundled Markdown and refuses to replace an existing `SKILL.md`. It does not install the CLI, edit Claude permissions, or start services. If an older binary lacks `--skill`, the command fails without publishing an empty skill.

Open a Claude Code session in that project. At the Claude Code prompt, invoke the skill with a bounded request:

```text
/parcela Inspect this checkout's status and services. Do not start, stop or restart any environment.
```

For a personal skill available across projects, use the same block with `skill_dir="$HOME/.claude/skills/parcela"`. Choose that scope explicitly. Do not replace a customized skill without reviewing its differences. After a CLI upgrade, inspect `parcela --skill` before updating the installed copy.

Without a compatible binary, [download the operating skill](../../.agents/skills/parcela/SKILL.md?raw=1) and save it as `.claude/skills/parcela/SKILL.md`. This is a standalone skill, not a published Claude plugin or marketplace installer. It covers operating configured environments; use the setup request above to configure a project first.

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
