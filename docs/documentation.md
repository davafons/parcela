# Build and publish the documentation site

The public site uses VitePress 1.6.4 for navigation, responsive layouts, theme selection and local search. The sidebar separates learning, task guides and reference. The docs-only Node package is separate from Parcela's runtime prerequisites.

## Edit the public sources

- Put public guides in `docs/site/` and list them in the page table in `docs/website/content.mjs`.
- Keep internal PRDs, research, plans and machine-specific verification records outside that directory.
- Update `docs/configuration.md` for the canonical configuration and CLI reference.
- Update `.agents/skills/parcela/SKILL.md` for the canonical operating instructions. The CLI embeds this file.
- Update `examples/docs-demo/` for the downloadable first-environment example.

`content.mjs` generates VitePress content and download assets from those canonical sources. It maps repository-relative Markdown links to public routes, including the task guide for transfer commands. The authored Markdown remains readable on GitHub. Generated directory-index pages retain the existing trailing-slash URLs. GitHub-style heading slugs preserve linked section anchors. Do not create a second copy of a canonical reference for the website.

The build refuses undeclared files in the public directory, public-source symlinks outside that root and canonical sources outside the repository. Only the page table and download table enter the build. Public pages use plain Markdown. Raw HTML rendering is disabled, and source includes, snippet imports and script blocks are refused. VitePress supplies the rendering assets; the build does not copy the whole repository.

## Install the build tools

Use Node 22.19 or newer and Python 3.12 or newer. From the repository root:

```sh
npm ci --prefix docs/website
```

The package lock pins transitive dependencies. A scoped override uses Vite 6.4.3 instead of VitePress 1.6.4's Vite 5 dependency to avoid known development-server advisories. Keep that override under the real build and search checks when updating the toolchain. `node_modules`, generated content, VitePress caches and `target/docs` are ignored build output. The command does not install Node, replace system packages or install Parcela's backends.

## Build and check

```sh
npm run build --prefix docs/website
python3 scripts/check_docs.py --output target/docs-check.json
```

The first command writes `target/docs` with `/parcela/` as its hosting base. The default canonical origin is `https://example.invalid`; set `DOCS_SITE` to the intended HTTPS origin for a publication build. Use `DOCS_BASE=/` for root hosting or a slash-delimited project path such as `/parcela/`. These variables configure output, not a deployment.

The checker builds separate temporary sites at `/`, `/parcela/` and `/docs/`. It checks actual HTML links, anchors, assets, canonical downloads and actual MiniSearch records for every public page. It loads the bundled index with MiniSearch and checks representative agent, checkpoint and frontend queries. It also verifies public-source refusal and serves all three builds through ephemeral loopback listeners. Every recorded listener must close before the check passes. The checker uses Python's standard library and the installed docs package.

For a private root-hosted preview, choose an available explicit test port:

```sh
DOCS_BASE=/ npm run build --prefix docs/website
python3 -m http.server "$DOCS_TEST_PORT" --bind 127.0.0.1 --directory target/docs
```

The dev command also binds to loopback. Restart it after editing canonical sources because generation happens when the configuration loads.

Open that local URL yourself. This command does not start a browser, change focus or expose a LAN interface. Artifact and HTTP checks do not prove responsive layout, theme interaction or search interaction in a browser.

## Verify the tutorial example

With explicit known Caddy and Process Compose binaries:

```sh
cargo build --locked
python3 scripts/check_docs_example.py \
  --caddy "$CADDY" --process-compose "$PROCESS_COMPOSE" \
  --output "$PRIVATE_REPORT"
```

The driver copies only the three demo inputs into disposable primary and preview directories. It uses isolated state, unprivileged ports and an explicit test CA. It verifies named frontend and API HTTPS, worker state and logs, generation restart, independent stop, retained source and closed listeners. Failed fixtures and their diagnostics remain for inspection. The driver does not install trust, run preparation, touch real dependencies or prove the installed standard front door.

## Publish the custom-domain site explicitly

The publication target is `https://parcela.davafons.com/docs/`. Cloudflare Workers serves static assets on a dedicated `parcela-docs` Worker in the `davafons` account. `docs/website/wrangler.jsonc` declares its account and custom domain. This deployment does not change repository visibility, the installed Parcela binary or a local Caddy instance.

Use authenticated Wrangler access to the selected account. The docs package pins Wrangler 4.150.0. Check the build without publishing:

```sh
npm run deploy --prefix docs/website -- --dry-run
```

The command builds a new artifact directory under `target/docs-deployment-*`. It sets the canonical origin to `https://parcela.davafons.com` and the base to `/docs/`. Only generated public site output enters the upload. The wrapper redirects `/` to `/docs/` and supplies a real 404 page. The VitePress build retains its rendered not-found content in the HTML, rather than requiring JavaScript to display the error. Bundled VitePress and Inter notices are available in `assets/third-party-notices.txt`. Each build uses a new directory, so removed pages cannot survive as stale deployment assets.

After reviewing the public artifact and obtaining deployment authorization:

```sh
npm run deploy --prefix docs/website
```

Wrangler uploads static assets and binds the explicitly declared custom domain. It can provision that domain's DNS and certificate. Before the first deployment, confirm that the hostname and Worker do not belong to another service. Do not approve replacement prompts for an unexpected existing binding.

Check the deployed HTTPS page, guide links, assets, search index and canonical downloads after publication. Verify the root redirect and an unknown path's 404 response. Local builds and a successful upload do not prove that public DNS and TLS are ready.

## Prepare optional GitHub Pages without implicit publication

The `Documentation` workflow builds and checks the site on relevant pull requests and main pushes. Its normal result is a downloadable `parcela-docs` artifact, not a deployment. CI uses Node 24 and Python 3.12.

Before publishing, review the generated site and every public source. Confirm that the repository and account support the intended Pages visibility. A private source repository does not necessarily make its Pages site private. Do not change repository visibility as a shortcut.

An authorized operator configures Pages to use GitHub Actions. After the reviewed change is on `main`, run the workflow manually with `publish` set to true. Only that explicit main-branch invocation uploads the Pages deployment artifact and enables the deployment job. The workflow does not enable Pages or alter repository settings itself.

The publication build uses the repository owner's GitHub Pages origin and `/parcela/`. If the intended host or base differs, update that configuration before deployment. Use the URL returned by the completed deployment. No address or deployed availability is promised by a local build. Live Pages deployment, permission behavior and public availability need separate verification.
