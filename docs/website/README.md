# Build the documentation site

VitePress builds `https://parcela.run` from this repository's Markdown. Use Node 22.19 or newer. Run the commands below from the repository root.

## Find the source

| Source | Purpose |
| --- | --- |
| `docs/site/docs.md` | User documentation index. |
| `docs/site/*.md` | Four primary tasks, reference, and an example app. |
| `docs/architecture.md`, `CONTRIBUTING.md` | Contributor documentation, also published on the site. |
| `.agents/skills/parcela/SKILL.md` | Operating skill embedded by `parcela --skill` and offered as a download. |
| `examples/docs-demo/` | Complete app from the tutorial. |
| `content.mjs` | Page registry, navigation groups, route mapping, downloads, and metadata. |

## Preview locally

Install dependencies:

```sh
npm ci --prefix docs/website
```

Start a loopback preview:

```sh
npm run dev --prefix docs/website -- --port 5173
```

Open `http://127.0.0.1:5173/`. The page registry watches listed source files. Restart the server to refresh search or after changing the registry.

To preview through Parcela instead, run `parcela up --detach` from this repository. Its setup hook installs dependencies. Open the environment URL it prints. This is a development preview, not the isolated verification harness.

## Add or edit a page

1. Choose a reader need using the [writing guidance](../../CONTRIBUTING.md#write-documentation).
2. Edit its Markdown source. For a new page, add a `[slug, title, source]` entry to `groups` in `content.mjs`.
3. Use relative Markdown links to registered pages. The build converts them to public routes. Links to unpublished files fail.
4. Use one H1. Optional YAML frontmatter sets `title` and `description`. Without those, metadata comes from the first heading and suitable paragraph.
5. Run the documentation check.

Includes and script blocks are rejected. Raw HTML and template expressions render as text. The landing page uses `layout: home` frontmatter and theme components.

The root route `/` is the landing page. `/docs/` is the documentation index. Other pages use `/docs/<slug>/`. When combining pages, add their old slugs to `redirects` and point them at a retained section. Retired guides are removed from the sidebar and search.

## Check the built site

```sh
npm run check --prefix docs/website
```

The command builds into `target/docs`. It verifies source coverage, links and anchors, legacy redirects, metadata, social cards, downloads, search, tutorial snippets, and the rendered Claude skill install block. Tool names gain local SVG marks at render time through `brands.mjs`. Code tokens stay unchanged and CSS wraps them within the reading column. It also checks telemetry endpoint validation.

For executable behavior, run:

```sh
scripts/check
scripts/check-live docs-example
```

The Python tests execute the Claude install block in a sandbox. The live suite runs the tutorial app through real Caddy in disposable environments with isolated state and test ports. Neither trusts a CA system-wide.

## Publishing pipeline

The `Documentation` workflow runs the site check and deploys `target/docs` when a push to `main` matches its path filters. `wrangler.jsonc` configures the Cloudflare Worker `parcela-site` and its static assets. Deployment uses the `CLOUDFLARE_API_TOKEN` repository secret.

The build copies the installer, schema, and downloads into the output. It also writes Cloudflare redirects and headers, `robots.txt`, `sitemap.xml`, and a 1200 by 630 social card per page. `og.mjs` renders the cards with resvg. The Worker serves static pages and the telemetry endpoint on `parcela.run`.

`DOCS_BASE` defaults to `/`. `DOCS_SITE` defaults to `https://parcela.run` and controls canonical URLs. `DOCS_OUTPUT` selects the output directory. The standard check uses the production base `/`.
