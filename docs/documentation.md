# Documentation site

The site at `https://parcela.davafons.com/docs/` is built with VitePress from Markdown in this repository.

## Sources

| File | Page |
|---|---|
| `docs/site/*.md` | Home, Getting started, worktrees, CLI guide, troubleshooting |
| `docs/configuration.md` | Configuration reference |
| `.agents/skills/parcela/SKILL.md` | Operating skill. The CLI embeds this file for `parcela --skill`. |
| `examples/docs-demo/` | The finished Getting started app, linked from that page |

`docs/website/content.mjs` lists every page and downloadable file. To add a page, put it in `docs/site/` and add it to `groups`. The build refuses files in `docs/site/` that are not listed, raw HTML, includes and `{{ }}` templates. Link between pages with relative Markdown links, such as `[CLI guide](cli.md)`. The build turns them into site routes.

A page under `docs/site/` can have YAML frontmatter. The home page uses it for the VitePress `layout: home` hero.

## Build and preview

Use Node 22.19 or newer and Python 3.12 or newer:

```sh
npm ci --prefix docs/website
npm run build --prefix docs/website
```

The site is written to `target/docs` with the base `/parcela/`. Set `DOCS_BASE=/` to build for the root, and `DOCS_SITE=https://...` for canonical links.

For live editing, start the `docs` service from the repository's own `parcela.yaml`:

```sh
parcela up --detach
parcela url
```

Without Parcela, run the VitePress dev server on a free port:

```sh
DOCS_BASE=/ npm run dev --prefix docs/website -- --port 5173
```

Search in the dev server does not refresh after edits. Restart it to rebuild the index.

## Check

```sh
python3 -m unittest discover -s tests -p 'test_docs.py'
python3 scripts/check_docs.py
```

`check_docs.py` builds the site at `/`, `/parcela/` and `/docs/`, then checks every link, anchor, asset, download and search record, and the theme's color contrast.

Two drivers run the docs against real Caddy and Process Compose, with a disposable state directory, test ports and a test CA:

```sh
cargo build --locked
python3 scripts/check_docs_example.py --caddy "$(which caddy)" --process-compose "$(which process-compose)" --output /tmp/docs-demo.json
python3 scripts/check_docs_dev.py --caddy "$(which caddy)" --process-compose "$(which process-compose)" --output /tmp/docs-dev.json
```

The first runs the Getting started app from `examples/docs-demo/` in a main checkout and a Git worktree. The second runs the docs dev server in two jj workspaces and checks hot reload over HTTPS.

## Deploy

Every push to `main` deploys the site through the `Documentation` workflow. It runs `npm run deploy --prefix docs/website` with the `CLOUDFLARE_API_TOKEN` secret. The token needs Workers Scripts:Edit and Workers Routes:Edit on the `davafons` account and Zone:Read on `davafons.com`.

The site is a Cloudflare Worker named `parcela-docs`, declared in `docs/website/wrangler.jsonc`. To check a deployment without publishing, or to deploy by hand:

```sh
npm run deploy --prefix docs/website -- --dry-run
npm run deploy --prefix docs/website
```
