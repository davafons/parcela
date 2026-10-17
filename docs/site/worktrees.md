# Parallel worktrees and agents

Every checkout of a project gets its own environment: its own ports, logs, processes and URLs. This page shows how checkouts are named, how to run several agents at once and how to keep their data apart.

## How checkouts are named

The main checkout of a project uses the project URL. Every other checkout puts its name in front:

| Checkout | Name | URL |
|---|---|---|
| Main checkout, or a plain directory | `primary` | `https://shop.localhost:1443` |
| jj workspace `signup` | `signup` | `https://signup-shop.localhost:1443` |
| Git worktree in directory `billing` | `billing` | `https://billing-shop.localhost:1443` |
| Any checkout started with `parcela up --name review` | `review` | `https://review-shop.localhost:1443` |

Other services add their name in front again, so the `api` service of `signup` is `https://api-signup-shop.localhost:1443`.

- The project name comes from the main checkout's directory. All jj workspaces and Git worktrees of a repository share it. Set `project: shop` in `parcela.yaml` to choose it yourself.
- Parcela remembers the name on the first `up`. Renaming a branch or a workspace keeps the same URL.
- A name that another checkout already uses is refused. Parcela never takes over another environment's URL.
- Use `parcela up --primary` to give a linked checkout the project URL, when no other checkout holds it.

## Run several agents at once

Give each agent its own worktree and start the app in it:

```sh
git worktree add ../signup && (cd ../signup && parcela up --detach)
git worktree add ../billing && (cd ../billing && parcela up --detach)
```

```text
✓ signup (shop) is running at https://signup-shop.localhost:1443
✓ billing (shop) is running at https://billing-shop.localhost:1443
```

With jj, use `jj workspace add ../signup`. Each agent then works only in its own directory. Inside it, `parcela url`, `parcela logs` and `parcela restart` act on that checkout and nothing else.

To act on a checkout from somewhere else, pass `--path`:

```sh
parcela --path ../billing url
```

```text
https://billing-shop.localhost:1443
```

See everything that runs on the machine:

```sh
parcela ls
```

```text
PROJECT  NAME     STATE    URL                                  PATH
shop     billing  ready    https://billing-shop.localhost:1443  /Users/you/code/billing
shop     signup   ready    https://signup-shop.localhost:1443   /Users/you/code/signup
shop     primary  stopped  https://shop.localhost:1443          /Users/you/code/shop
```

## Keep data separate

Parcela separates ports, URLs, processes and logs. It does not copy or separate your data. If two checkouts read the same `.env` and the same database, they share it.

Each environment has a private runtime directory in `PARCELA_RUNTIME_DIR`, with `data`, `tmp`, `cache` and `log` inside. Put per-checkout files there. For SQLite, point the database file at it, in whatever variable your app reads:

```yaml
environment:
  DATABASE_PATH: ${PARCELA_RUNTIME_DIR}/data/app.sqlite3
```

For a server database, run one per checkout as a worker on a named port. This Postgres worker creates its data directory on first start:

```yaml
web: python3 -m http.server $PORT --bind 127.0.0.1
workers:
  db: sh -c 'test -d "$PARCELA_RUNTIME_DIR/data/pg" || initdb -D "$PARCELA_RUNTIME_DIR/data/pg"; exec postgres -D "$PARCELA_RUNTIME_DIR/data/pg" -h 127.0.0.1 -p "$PARCELA_PORT_DB" -k /tmp'
ports: [db]
environment:
  DATABASE_URL: postgres://127.0.0.1:${PARCELA_PORT_DB}/postgres
tasks:
  psql:
    argv: [sh, -c, 'psql "$DATABASE_URL" "$@"', psql]
```

Each checkout now has its own cluster on its own port:

```sh
parcela task psql -- -tAc 'show port'
(cd ../search && parcela task psql -- -tAc 'show port')
```

```text
26341
22030
```

Seeding and migrations stay your project's job. Run them with your usual commands, or define them as tasks and run them with `parcela task`.

## Clean up

```sh
parcela down --all            # stop every environment of this project
parcela down --all-projects   # stop every environment on the machine
git worktree remove ../billing
parcela rm --prune            # forget checkouts whose directory is gone
```

`down` keeps everything on disk. `rm` forgets a stopped environment and frees its URLs. It keeps the runtime directory, so a database there survives until you delete it yourself.

## Set up a coding agent

Parcela ships instructions for coding agents inside the binary. Print them with:

```sh
parcela --skill
```

The output is Markdown. It tells the agent how to pick the right checkout, start it with `--detach`, read `--json` output, find URLs and logs and stop only its own environment. You can also [read the operating skill](../../.agents/skills/parcela/SKILL.md) here or [download it](../../.agents/skills/parcela/SKILL.md?raw=1).

### Install the skill for Claude Code

[Claude Code discovers skills](https://code.claude.com/docs/en/skills) in `.claude/skills/<name>/SKILL.md`. Run this from the project root:

```sh
(
	set -eu
	skill_root=.claude
	skill_dir="$skill_root/skills/parcela"
	for skill_path in "$skill_root" "$skill_root/skills" "$skill_dir"; do
		if test -L "$skill_path"; then
			printf 'Refusing symlinked skill directory: %s\n' "$skill_path" >&2
			exit 1
		fi
	done
	mkdir -p "$skill_dir"
	stage=$(mktemp -d "$skill_dir/.install.XXXXXX")
	trap 'rm -f "$stage/SKILL.md"; rmdir "$stage"' EXIT
	parcela --skill > "$stage/SKILL.md"
	test -s "$stage/SKILL.md"
	ln "$stage/SKILL.md" "$skill_dir/"
)
```

It writes `.claude/skills/parcela/SKILL.md` and refuses to replace an existing one. To install it for every project, replace the `skill_root=.claude` line with `skill_root="$HOME/.claude"`. After upgrading Parcela, compare `parcela --skill` with the installed copy and update it.

Then ask Claude Code something bounded:

```text
/parcela Show this checkout's status and URLs. Don't start or stop anything.
```

### Ask an agent to configure a project

Paste this into your agent to have it write `parcela.yaml` for a project:

```text
Configure this checkout for Parcela.

Read `parcela --skill` and https://parcela.davafons.com/docs/configuration/.
Look at how the project starts its dev servers, workers and databases today.
Don't read .env files or credentials.

Write a minimal parcela.yaml with the project's real commands. Every HTTP
service must listen on 127.0.0.1:$PORT. Put databases and other ports under
`ports:` and per-checkout files under $PARCELA_RUNTIME_DIR.
Ask me before choosing how databases, uploads or queues are shared.

Run `parcela doctor`. Don't start services, run migrations or install anything.
Show me the diff and the commands to start it.
```
