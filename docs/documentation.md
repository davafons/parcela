# Build and publish the documentation site

The public site uses VitePress 1.6.4 for navigation, responsive layouts, theme selection and local search. The sidebar separates learning, task guides and reference. The docs-only Node package is separate from Parcela's runtime prerequisites.

## Edit the public sources

- Put public guides in `docs/site/` and list them in the page table in `docs/website/content.mjs`.
- Keep internal PRDs, research, plans and machine-specific verification records outside that directory.
- Update `docs/configuration.md` for the canonical configuration and CLI reference.
- Update `.agents/skills/parcela/SKILL.md` for the canonical operating instructions. The CLI embeds this file.
- Update `examples/docs-demo/` for the downloadable first-environment example.

`content.mjs` generates VitePress content and download assets from those canonical sources. It maps repository-relative Markdown links to public routes. The authored Markdown remains readable on GitHub. Generated directory-index pages retain the existing trailing-slash URLs. GitHub-style heading slugs preserve linked section anchors. Do not create a second copy of a canonical reference for the website.

The build refuses undeclared files in the public directory, public-source symlinks outside that root and canonical sources outside the repository. Only the page table and download table enter the build. Public pages use plain Markdown. Raw HTML rendering is disabled, and source includes, snippet imports and script blocks are refused. VitePress supplies the rendering assets; the build does not copy the whole repository.

## Install the build tools

Use Node 22.19 or newer and Python 3.12 or newer. From the repository root:

```sh
npm ci --prefix docs/website
```

The package lock pins transitive dependencies. A scoped override uses Vite 6.4.3 instead of VitePress 1.6.4's Vite 5 dependency to avoid known development-server advisories. Keep that override under the real build and search checks when updating the toolchain. `node_modules`, generated content, VitePress caches and `target/docs` are ignored build output. The command does not install Node, replace system packages or install Parcela's backends.

## Develop the docs with Parcela

The root `parcela.yaml` starts one VitePress service named `docs`. It uses the checkout's allocated loopback port and canonical HTTPS origin. Port conflicts fail instead of selecting another port. Each checkout keeps its generated sources and cache under `docs/website/.generated/`. No database or application preparation is needed.

Install the pinned docs package separately with the command above. Use a Parcela binary compatible with your registry, an already working front door and an available Process Compose binary. Do not open an installed registry with a newer binary while older controllers still use it. If proxy status fails, coordinate host setup instead of changing listeners, trust or forwarding here.

After authorizing adoption and startup, run these commands from the intended checkout root. Choose a different preview label for each checkout:

```sh
CHECKOUT=$(pwd -P)
parcela --path "$CHECKOUT" config check
parcela proxy status
parcela --path "$CHECKOUT" workspace adopt --role preview --label docs-review
parcela --path "$CHECKOUT" up --detach
parcela --path "$CHECKOUT" url
```

Open the returned URL. The recipe does not open a browser or change focus. Designating a primary checkout is a separate explicit choice.

Edit the canonical Markdown, configuration reference or operating skill, not generated files. A separate Node file watcher validates all declared inputs before changing the generated snapshot. The source watcher passes only validated generated pages to VitePress rendering and search. Native VitePress handles Markdown and stylesheet hot updates. Changes to downloadable assets can reload the page. Invalid templates or undeclared public inputs leave the previous valid generated content in place and report an error. Correct the source to recover.

**Development search has a known limitation in VitePress 1.6.4.** Its local-search index stays stale after edits. Restart the selected graph and reload the page to refresh search. Production builds create a fresh index. Restart after changing source directory layout as well.

```sh
parcela --path "$CHECKOUT" services
parcela --path "$CHECKOUT" logs docs --tail 50
parcela --path "$CHECKOUT" restart --detach
```

The manifest also declares finite build and check tasks. They require the already installed docs package and run only when requested:

```sh
parcela --path "$CHECKOUT" task docs-build
parcela --path "$CHECKOUT" task docs-check
parcela --path "$CHECKOUT" down
```

The build task writes this checkout's `target/docs`. The check task runs the standalone three-base checker below. Stopping the graph preserves source and generated files. It does not stop other checkouts.

For direct development without Parcela, choose an unused explicit unprivileged port:

```sh
DOCS_BASE=/ npm run dev --prefix docs/website -- --port "$DOCS_TEST_PORT"
```

This also binds to loopback, refuses a port conflict and watches the canonical sources. It does not provide named HTTPS or establish browser trust.

## Build and check

```sh
npm run build --prefix docs/website
python3 scripts/check_docs.py --output target/docs-check.json
```

The first command writes `target/docs` with `/parcela/` as its hosting base. The default canonical origin is `https://example.invalid`; set `DOCS_SITE` to the intended HTTPS origin for a publication build. Use `DOCS_BASE=/` for root hosting or a slash-delimited project path such as `/parcela/`. These variables configure output, not a deployment.

The checker builds separate temporary sites at `/`, `/parcela/` and `/docs/`. It checks actual HTML links, anchors, assets, canonical downloads and actual MiniSearch records for every public page. It loads the bundled index with MiniSearch and checks representative agent, worktree and frontend queries. It also verifies public-source refusal and serves all three builds through ephemeral loopback listeners. Every recorded listener must close before the check passes. The checker uses Python's standard library and the installed docs package.

For a private root-hosted preview, choose an available explicit test port:

```sh
DOCS_BASE=/ npm run build --prefix docs/website
python3 -m http.server "$DOCS_TEST_PORT" --bind 127.0.0.1 --directory target/docs
```

This serves a compiled artifact. Rebuild it after source edits, or use the live development recipe above.

Open that local URL yourself. This command does not start a browser, change focus or expose a LAN interface. Artifact and HTTP checks do not prove responsive layout, theme interaction or search interaction in a browser.

## Verify live docs development

With jj, the installed docs package and explicit known Caddy and Process Compose binaries:

```sh
cargo build --locked
python3 scripts/check_docs_dev.py \
	--caddy "$CADDY" --process-compose "$PROCESS_COMPOSE" \
	--output "$PRIVATE_REPORT"
```

The driver copies declared inputs into disposable sibling jj workspaces. It proves read-only discovery, separate primary and preview graphs, strict port refusal, named HTTPS and native HMR messages over WSS using a private test CA. It checks canonical Markdown, skill downloads, CSS, source refusal and recovery, the explicit build task, search refresh after restart, independent stop and listener closure. Failed fixtures and diagnostics remain available. These protocol checks do not prove browser trust, standard ports or browser DOM updates. Browser execution is a separate check.

## Verify the tutorial example

With explicit known Caddy and Process Compose binaries:

```sh
cargo build --locked
python3 scripts/check_docs_example.py \
  --caddy "$CADDY" --process-compose "$PROCESS_COMPOSE" \
  --output "$PRIVATE_REPORT"
```

The driver copies only the three demo inputs into disposable primary and preview directories. It uses isolated state, unprivileged ports and an explicit test CA. It verifies named frontend and API HTTPS, worker state and logs, generation restart, independent stop, retained source and closed listeners. Failed fixtures and their diagnostics remain for inspection. The driver does not install trust, run preparation, touch real dependencies or prove the installed standard front door.

## Publish the custom-domain site

The publication target is `https://parcela.davafons.com/docs/`. Cloudflare Workers serves static assets on a dedicated `parcela-docs` Worker in the `davafons` account. `docs/website/wrangler.jsonc` declares its account and custom domain. This deployment does not change repository visibility, the installed Parcela binary or a local Caddy instance.

Use authenticated Wrangler access to the selected account. The docs package pins Wrangler 4.150.0. Check the build without publishing:

```sh
npm run deploy --prefix docs/website -- --dry-run
```

The command builds a new artifact directory under `target/docs-deployment-*`. It sets the canonical origin to `https://parcela.davafons.com` and the base to `/docs/`. Only generated public site output enters the upload. The wrapper redirects `/` to `/docs/` and supplies a real 404 page. The VitePress build retains its rendered not-found content in the HTML, rather than requiring JavaScript to display the error. Bundled VitePress and Inter notices are available in `assets/third-party-notices.txt`. Each build uses a new directory, so removed pages cannot survive as stale deployment assets.

Every push to `main` deploys automatically through the `Documentation` workflow. To deploy by hand:

```sh
npm run deploy --prefix docs/website
```

Wrangler uploads static assets and binds the explicitly declared custom domain. It can provision that domain's DNS and certificate. Before the first deployment, confirm that the hostname and Worker do not belong to another service. Do not approve replacement prompts for an unexpected existing binding.

Check the deployed HTTPS page, guide links, assets, search index and canonical downloads after publication. Verify the root redirect and an unknown path's 404 response. Local builds and a successful upload do not prove that public DNS and TLS are ready.

## Continuous deployment

The `Documentation` workflow builds and checks the site on relevant pull requests and pushes to `main`. CI uses Node 24 and Python 3.12. On `main`, a `deploy` job then runs `npm run deploy --prefix docs/website` with the `CLOUDFLARE_API_TOKEN` repository secret. Create that token in Cloudflare with Workers Scripts:Edit and Workers Routes:Edit on the `davafons` account and Zone:Read on `davafons.com`. Without the secret the deploy job fails and names it.
