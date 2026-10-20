import { existsSync, mkdirSync, readFileSync, rmSync, watch, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderCard, renderIcon } from './og.mjs';

const project = dirname(fileURLToPath(import.meta.url));
export const repository = resolve(project, '../..');
export const srcDir = join(project, '.generated/src');

export const groups = [
  { label: 'Introduction', pages: [
    ['index', 'Home', 'docs/site/index.md'],
    ['getting-started', 'Getting started', 'docs/site/getting-started.md'],
    ['faq', 'FAQ', 'docs/site/faq.md'],
  ] },
  { label: 'Guides', pages: [
    ['worktrees', 'Parallel worktrees and agents', 'docs/site/worktrees.md'],
    ['frameworks', 'Frameworks and data', 'docs/site/frameworks.md'],
    ['cli', 'CLI guide', 'docs/site/cli.md'],
    ['troubleshooting', 'Troubleshooting', 'docs/site/troubleshooting.md'],
  ] },
  { label: 'Reference', pages: [
    ['configuration', 'Configuration', 'docs/site/configuration.md'],
    ['operating-skill', 'Operating skill', '.agents/skills/parcela/SKILL.md'],
  ] },
];
export const pages = groups.flatMap((group) => group.pages);
// The landing page is the site root; every other page lives under /docs/.
export const route = (slug) => (slug === 'index' ? '/' : `/docs/${slug}/`);

export const downloads = {
  'parcela-skill.txt': '.agents/skills/parcela/SKILL.md',
  'logo.svg': 'assets/brand/logo.svg',
  'third-party-notices.txt': 'docs/website/THIRD_PARTY_NOTICES.txt',
};

const routes = new Map(pages.map(([slug, , source]) => [join(repository, source), route(slug)]));
const raw = new Map(Object.entries(downloads).map(([name, source]) => [join(repository, source), `/assets/${name}`]));

function render(source) {
  const text = readFileSync(join(repository, source), 'utf8');
  if (/<!--\s*@include:|<script/i.test(text)) throw new Error(`Includes and scripts are not allowed: ${source}`);
  const markdown = source.startsWith('docs/site/') ? text : text.replace(/^---\n[\s\S]*?\n---\n/, '');
  return markdown.replace(/\]\(([^)]+)\)/g, (match, reference) => {
    if (/^(?:[a-z]+:|#|\/)/i.test(reference)) return match;
    const [file, suffix = ''] = reference.split(/(?=[?#])/);
    const target = resolve(dirname(join(repository, source)), file);
    const link = suffix === '?raw=1' ? raw.get(target) : routes.get(target)?.concat(suffix);
    if (!link) throw new Error(`Link to an unpublished file: ${reference} from ${source}`);
    return `](${link})`;
  });
}

// A page's title and description for search results and link previews: the frontmatter's
// when it has them, else the first heading (or the sidebar name, when the heading is just
// "Parcela") and the first paragraph that does not lead into a list or code block.
function describe(markdown, name) {
  const front = /^---\n([\s\S]*?)\n---\n/.exec(markdown)?.[1] ?? '';
  const field = (name) => new RegExp(`^${name}: (.+)$`, 'm').exec(front)?.[1].trim();
  const body = markdown.replace(/^---\n[\s\S]*?\n---\n/, '');
  const plain = (text) => text
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/`([^`]*)`/g, '$1')
    .replace(/\*\*?([^*]+)\*\*?/g, '$1')
    .replace(/\s+/g, ' ')
    .trim();
  const heading = plain(/^# (.+)$/m.exec(body)?.[1] ?? '');
  const title = field('title') ?? (heading === 'Parcela' ? name : heading);
  const paragraph = body.split(/\n{2,}/).map((block) => block.trim())
    .find((block) => /^[A-Za-z`*[]/.test(block) && !block.startsWith('```') && !block.endsWith(':'));
  let description = field('description') ?? plain(paragraph ?? '');
  if (description.length > 180) {
    const cut = description.slice(0, 180);
    description = cut.includes('. ') ? cut.slice(0, cut.lastIndexOf('. ') + 1) : `${cut.replace(/\s+\S*$/, '')}…`;
  }
  return { title, description };
}

export const meta = new Map();
export const card = (slug) => `/og/${slug}.png`;

function write(path, bytes) {
  if (existsSync(path) && readFileSync(path).equals(Buffer.from(bytes))) return;
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, bytes);
}

export function prepareContent({ clean = true } = {}) {
  const files = pages.map(([slug, , source]) => [join(srcDir, slug === 'index' ? 'index.md' : `docs/${slug}/index.md`), render(source)]);
  if (clean) rmSync(srcDir, { recursive: true, force: true });
  for (const [path, markdown] of files) write(path, markdown);
  for (const [name, source] of Object.entries(downloads)) write(join(srcDir, 'public/assets', name), readFileSync(join(repository, source)));
  if (clean || !existsSync(join(srcDir, 'public/apple-touch-icon.png'))) write(join(srcDir, 'public/apple-touch-icon.png'), renderIcon(180));
  // /docs/ has no page of its own; send it to the first one.
  const first = route('getting-started');
  write(join(srcDir, 'public/docs/index.html'), `<!doctype html><meta charset="utf-8"><title>Parcela docs</title><meta http-equiv="refresh" content="0; url=${first}"><link rel="canonical" href="${first}"><meta name="robots" content="noindex"><a href="${first}">Parcela docs</a>\n`);
  for (const [i, [slug, name]] of pages.entries()) {
    const page = describe(files[i][1], name);
    if (meta.get(slug)?.title === page.title && meta.get(slug)?.description === page.description) continue;
    meta.set(slug, page);
    write(join(srcDir, 'public', card(slug)), renderCard(page));
  }
}

export const watchSources = {
  name: 'parcela-sources',
  apply: 'serve',
  configureServer(server) {
    const sources = new Set([...routes.keys(), ...raw.keys()]);
    const watchers = [...new Set([...sources].map(dirname))].map((directory) => watch(directory, (_event, name) => {
      if (!name || !sources.has(join(directory, name.toString()))) return;
      try {
        prepareContent({ clean: false });
      } catch (error) {
        server.config.logger.error(error.message);
      }
    }));
    server.httpServer?.once('close', () => watchers.forEach((watcher) => watcher.close()));
  },
};
