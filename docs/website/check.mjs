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

console.log(`docs check passed: ${pages.length} pages, ${Object.keys(downloads).length} downloads, search and install block at ${base} in ${output}`);
