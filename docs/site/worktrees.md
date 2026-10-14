# Manage primary and preview worktrees

## Select the intended checkout

Start with an existing configured checkout. Create jj workspaces or Git worktrees through the project's VCS workflow, not through an inferred Parcela action. Parcela also supports plain configured directories.

```sh
parcela --path "$CHECKOUT" workspace discover
parcela --path "$CHECKOUT" workspace list
```

Discovery reads sibling VCS metadata. It does not adopt a directory, create a worktree or change history. In a colocated repository, jj takes precedence over Git. A missing source path stays recorded.

For explicit machine-wide inventory, including from outside a project:

```sh
parcela workspace list --all
```

Read the actual environment IDs, source paths, roles and observed states. Do not select an environment from a branch name or focused terminal alone.

## Adopt an existing preview

Read the preview's project instructions and `parcela.yaml`. Keep the same `project.id` and project name across copies. Choose an explicit label and a project-owned data policy.

> **Names are not data isolation.**
>
> Do not launch a preview whose dotenv, database, uploads or queue still point at primary resources unless that sharing is explicitly intended. Parcela does not clone, migrate or seed those resources. A project launcher may refuse previews until bindings are configured.

After the required bindings are in place:

```sh
parcela --path "$PREVIEW" config check
parcela --path "$PREVIEW" workspace adopt --role preview --label review
parcela --path "$PREVIEW" context
```

For project `my-app`, the default service reserves `review-my-app.localhost`. An API prefix reserves `api-review-my-app.localhost`. Adoption refuses conflicts with another environment's base or secondary names rather than taking them over.

The primary uses `my-app.localhost`, regardless of its label. A role does not change because a bookmark moves. Repeating identical adoption keeps the environment identity; implicit role or path rebind is not supported.

## Operate from another directory

Set `ENVIRONMENT_ID` to the exact UUID returned by adoption or inventory:

```sh
parcela --workspace "$ENVIRONMENT_ID" status
parcela --workspace "$ENVIRONMENT_ID" up --detach
parcela --workspace "$ENVIRONMENT_ID" services
parcela --workspace "$ENVIRONMENT_ID" url --service backend
```

Use an actual declared service name. `--workspace` and `--path` conflict. Keep the same `--state-dir` prefix on all calls when using a nondefault registry. UUID selection does not forward a command to another machine.

## Restart or stop only that environment

```sh
parcela --workspace "$ENVIRONMENT_ID" restart --detach
parcela --workspace "$ENVIRONMENT_ID" status
parcela --workspace "$ENVIRONMENT_ID" down
```

Restart operates on the whole graph. It preflights the replacement, then starts a new generation with a new nonce and endpoint allocation. Stop preserves source and data. Stale stop requests cannot silently act on a successor generation.

Status and qualified stop can still use an environment ID when its source has gone missing. Starting or restarting requires its current valid source manifest. Do not delete an uncertain record as a remedy.

## Change service reservations explicitly

Stop the selected environment first. Edit its service names or prefixes, then re-adopt with the same role and label. Resolve any reported collision before startup.

Stopped hostnames stay reserved. Source moves, role changes and project renames do not have an automatic rebind command. Do not simulate one by modifying the private registry.
