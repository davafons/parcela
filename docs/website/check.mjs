import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { downloads, pages, repository, route } from './content.mjs';

const base = '/';
const output = process.env.DOCS_OUTPUT ?? join(repository, 'target/docs');
execFileSync(join(import.meta.dirname, 'node_modules/.bin/vitepress'), ['build'], {
  cwd: import.meta.dirname, stdio: 'inherit', env: { ...process.env, DOCS_BASE: base, DOCS_OUTPUT: output },
});

const html = (slug) => readFileSync(join(output, route(slug), 'index.html'), 'utf8');
const ids = new Map(pages.map(([slug]) => [`${base}${route(slug).slice(1)}`, new Set([...html(slug).matchAll(/ id="([^"]+)"/g)].map((match) => match[1]))]));
for (const [slug, , source] of pages) {
  assert.equal(html(slug).match(/<h1[\s>]/g)?.length, 1, `${source} renders one page heading`);
  assert.ok(!html(slug).includes(homedir()), `${source} leaks no local path`);
  for (const [, path, anchor] of html(slug).matchAll(/href="([^"#]*)#([^"]+)"/g)) {
    const target = path || `${base}${route(slug).slice(1)}`;
    if (ids.has(target)) assert.ok(ids.get(target).has(decodeURIComponent(anchor)), `${source} links to an existing anchor ${path}#${anchor}`);
  }
}

for (const [name, source] of Object.entries(downloads)) {
  assert.deepEqual(readFileSync(join(output, 'assets', name)), readFileSync(join(repository, source)), `download ${name} matches ${source}`);
}

// Search engines and link previews: one description per page, a canonical URL, and a
// 1200x630 card that exists in the build.
const site = process.env.DOCS_SITE ?? 'https://parcela.run';
const head = (slug) => html(slug).split('</head>')[0];
const property = (slug, name) => [...head(slug).matchAll(new RegExp(`<meta (?:name|property)="${name}" content="([^"]*)"`, 'g'))].map((match) => match[1]);
const descriptions = new Set();
const sitemap = readFileSync(join(output, 'sitemap.xml'), 'utf8');
for (const [slug, , source] of pages) {
  const url = `${site}${base}${route(slug).slice(1)}`;
  const [description, ...extra] = property(slug, 'description');
  assert.ok(description && !extra.length, `${source} has one meta description`);
  assert.ok(!descriptions.has(description), `${source} has its own description`);
  descriptions.add(description);
  assert.ok(head(slug).includes(`<link rel="canonical" href="${url}">`), `${source} is canonical at ${url}`);
  assert.deepEqual(property(slug, 'og:url'), [url], `${source} has og:url`);
  for (const name of ['og:title', 'og:description', 'twitter:card', 'twitter:image']) assert.equal(property(slug, name).length, 1, `${source} has ${name}`);
  const [image] = property(slug, 'og:image');
  assert.ok(image?.startsWith(`${site}${base}`), `${source} has an absolute og:image`);
  const png = readFileSync(join(output, image.slice(`${site}${base}`.length)));
  assert.deepEqual([png.readUInt32BE(16), png.readUInt32BE(20)], [1200, 630], `${source} card is 1200x630`);
  assert.ok(sitemap.includes(`<loc>${url}</loc>`), `the sitemap lists ${url}`);
}
assert.ok(readFileSync(join(output, 'robots.txt'), 'utf8').includes(`Sitemap: ${site}${base}sitemap.xml`), 'robots.txt points at the sitemap');

const chunks = join(output, 'assets/chunks');
const index = readdirSync(chunks).filter((name) => name.startsWith('@localSearchIndex'));
assert.equal(index.length, 1, 'one local search index');
const { default: serialized } = await import(join(chunks, index[0]));
const MiniSearch = createRequire(createRequire(import.meta.url).resolve('vitepress/package.json'))('minisearch');
const search = MiniSearch.loadJSON(serialized, { fields: ['title', 'titles', 'text'], storeFields: ['title', 'titles'] });
for (const [slug, title, source] of pages) {
  const front = /^---\n([\s\S]*?)\n---\n/.exec(readFileSync(join(repository, source), 'utf8'))?.[1] ?? '';
  if (/^search: false$/m.test(front)) continue;
  const hits = search.search(title, { prefix: true }).map((hit) => hit.id.split('#')[0]);
  assert.ok(hits.includes(`${base}${route(slug).slice(1)}`), `searching "${title}" finds ${source}`);
}

const guide = readFileSync(join(repository, 'docs/site/worktrees.md'), 'utf8');
const install = /### Install the skill for Claude Code\n[\s\S]*?```sh\n([\s\S]*?)\n```/.exec(guide)[1];
const entities = { '&lt;': '<', '&gt;': '>', '&quot;': '"', '&#39;': "'", '&amp;': '&' };
const blocks = [...html('worktrees').matchAll(/<pre[^>]*>([\s\S]*?)<\/pre>/g)]
  .map(([, code]) => code.replace(/<[^>]+>/g, '').replace(/&(?:lt|gt|quot|#39|amp);/g, (entity) => entities[entity]).trimEnd());
assert.ok(blocks.includes(install), 'the rendered Claude skill install block matches docs/site/worktrees.md');

console.log(`docs check passed: ${pages.length} pages with cards and metadata, ${Object.keys(downloads).length} downloads, search and install block at ${base} in ${output}`);
