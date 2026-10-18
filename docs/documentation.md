# Documentation site

The site at `https://parcela.davafons.com/docs/` is built with VitePress from Markdown in this repository.

## Sources

| File | Page |
|---|---|
| `docs/site/*.md` | Home, Getting started, worktrees, CLI guide, troubleshooting |
| `docs/configuration.md` | Configuration reference |
| `.agents/skills/parcela/SKILL.md` | Operating skill. The CLI embeds this file for `parcela --skill`. |
| `examples/docs-demo/` | The finished Getting started app, linked from that page |

`docs/website/content.mjs` lists every page and downloadable file. To add a page, put it in `docs/site/` and add it to `groups`. Link between pages with relative Markdown links, such as `[CLI guide](cli.md)`, so they also work on GitHub. The build turns them into site routes and fails on a link to a file that is not published. It also refuses includes and `<script>` blocks. Raw HTML and `{{ }}` render as plain text.

A page under `docs/site/` can have YAML frontmatter. The home page uses it for the VitePress `layout: home` hero.

## Build, preview and check

Use Node 22.19 or newer. The repository's own `parcela.yaml` runs the dev server, so each worktree previews its docs at its own URL:

```sh
parcela up --detach
```

The first `up` installs the packages. The site is at the environment's URL, such as `https://parcela.localhost:1443`, and reloads when a listed source changes. Without Parcela:

```sh
npm ci --prefix docs/website
npm run dev --prefix docs/website -- --port 5173
npm run check --prefix docs/website
```

That serves `http://127.0.0.1:5173/docs/`. Search does not refresh until you restart it.

`npm run check` builds the site once into `target/docs` at the production base `/docs/`. The build fails on a dead link. The check then confirms that every page has one heading and only existing anchors, that every searchable page is in the search index, that the downloads match their sources and that the rendered Claude skill install block matches `docs/site/worktrees.md`. `tests/test_skill_install.py` runs that block in a sandbox. `scripts/check-live docs-example` runs the Getting started app against real Caddy and Process Compose.

## Deploy

Every push to `main` that touches the docs runs `npm run check` and publishes its output to GitHub Pages through the `Documentation` workflow. `DOCS_BASE` (default `/docs/`) and `DOCS_SITE` (default `https://parcela.davafons.com`) set the base path and canonical origin. The workflow adds a root page that redirects to `/docs/` and a copy of the 404 page. The custom domain `parcela.davafons.com` is set in the repository's Pages settings and needs a DNS `CNAME` to `davafons.github.io`.
