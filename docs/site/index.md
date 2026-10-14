# Run development services at named HTTPS URLs

Parcela connects a checkout to its service set. Keep a primary environment and several previews identifiable without managing browser-facing port numbers.

```text
Project
├── Primary checkout
│   ├── frontend → my-app.localhost
│   ├── backend  → api-my-app.localhost
│   └── worker   → no HTTP route
└── Preview checkout, label review
    ├── frontend → review-my-app.localhost
    └── backend  → api-review-my-app.localhost
```

Caddy handles HTTPS. Process Compose handles processes, dependencies, logs and shutdown. Parcela owns environment identity, endpoint reservations, readiness and lifecycle coordination. Your project owns commands and data policy.

> **Experimental.**
>
> These guides describe the [`docs-site` development branch](https://github.com/davafons/parcela/tree/docs-site), not a finished installer. Its service-set and bundled-skill changes are not merged into `main` yet. A reserved URL is not proof that a service is reachable. Separate names and runtime directories do not isolate application databases or uploads.

## Choose your next task

| You want to | Read |
|---|---|
| Build Parcela and prepare HTTPS | [Build and host setup](install.md) |
| Run your first frontend, API and worker | [First environment](first-environment.md) |
| Keep primary and preview checkouts separate | [Manage worktrees](worktrees.md) |
| Declare processes, ports and readiness | [Configure service sets](services.md) |
| Teach a coding agent to use Parcela | [Work with an agent](agents.md) |
| Diagnose a failed start or lost controller | [Troubleshoot and recover](troubleshooting.md) |
| Capture a paused copy, not active execution | [Paused checkpoints](transfer.md) |
| Look up exact fields and commands | [Configuration and CLI](../configuration.md) |
| Check what is not implemented or certified | [Supported scope](limits.md) |

## Keep the ownership boundary clear

Adoption registers an existing directory. It does not create a worktree, move a branch, prepare a database or start a service.

`up` starts one explicitly configured native graph. All declared HTTP services must become ready before its group routes open. `down` stops that selected environment without deleting source or data. `restart` creates a new generation. Qualified `recover` can reattach to a surviving native graph without replaying startup.

Ordinary commands do not install backends, trust a CA, take over listeners or expose a LAN interface. Start with your project's instructions and its `parcela.yaml`, not a guessed framework recipe.
