import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const MiniSearch = createRequire(require.resolve('vitepress/package.json'))('minisearch');
const source = readFileSync(process.argv[2]);
const { default: serialized } = await import(`data:text/javascript;base64,${source.toString('base64')}`);
const data = JSON.parse(serialized);
const search = MiniSearch.loadJSON(serialized, {
  fields: ['title', 'titles', 'text'],
  storeFields: ['title', 'titles'],
});
console.log(JSON.stringify({
  documentCount: search.documentCount,
  documents: Object.entries(data.documentIds).map(([key, id]) => ({ id, ...data.storedFields[key] })),
  queries: Object.fromEntries(['Claude', 'worktree', 'frontend'].map(term => [
    term, search.search(term, { prefix: true }).map(result => result.id),
  ])),
}));
