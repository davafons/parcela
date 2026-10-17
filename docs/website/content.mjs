import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, watch, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const project = dirname(fileURLToPath(import.meta.url));
const repository = resolve(project, '../..');

export const groups = [
  { label: 'Introduction', pages: [
    ['index', 'Home', 'docs/site/index.md'],
    ['getting-started', 'Getting started', 'docs/site/getting-started.md'],
  ] },
  { label: 'Guides', pages: [
    ['worktrees', 'Parallel worktrees and agents', 'docs/site/worktrees.md'],
    ['cli', 'CLI guide', 'docs/site/cli.md'],
    ['troubleshooting', 'Troubleshooting', 'docs/site/troubleshooting.md'],
  ] },
  { label: 'Reference', pages: [
    ['configuration', 'Configuration', 'docs/configuration.md'],
    ['operating-skill', 'Operating skill', '.agents/skills/parcela/SKILL.md'],
  ] },
];

export const downloads = {
  'parcela-skill.txt': '.agents/skills/parcela/SKILL.md',
  'logo.svg': 'assets/brand/logo.svg',
  'third-party-notices.txt': 'docs/website/THIRD_PARTY_NOTICES.txt',
};

export function validatePublicSources(directory) {
  const root = realpathSync(directory);
  const allowed = new Set(groups.flatMap(({ pages }) => pages)
    .map(([, , source]) => source).filter((source) => source.startsWith('docs/site/'))
    .map((source) => source.slice('docs/site/'.length)));
  function visit(path) {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      const source = join(path, entry.name);
      const name = relative(root, source);
      const resolved = relative(root, realpathSync(source));
      if (resolved === '..' || resolved.startsWith(`..${sep}`)) {
        throw new Error(`Public documentation source escapes its root: ${name}`);
      }
      if (entry.isDirectory()) visit(source);
      else if (!allowed.has(name)) throw new Error(`Undeclared public documentation source: ${name}`);
    }
  }
  visit(root);
}

export function validateMarkdownSource(markdown, source) {
  if (/<!--\s*@include:|<<<|<script(?:\s|>)|\{\{/i.test(markdown)) {
    throw new Error(`Public Markdown cannot load extra sources or execute templates: ${source}`);
  }
}

function sourceBytes(source) {
  const path = join(repository, source);
  const resolved = relative(repository, realpathSync(path));
  if (resolved === '..' || resolved.startsWith(`..${sep}`)) {
    throw new Error(`Canonical documentation source escapes the repository: ${source}`);
  }
  return readFileSync(path);
}

export function prepareContent(base, srcDir) {
  validatePublicSources(join(repository, 'docs/site'));
  const generatedRoot = join(project, '.generated');
  mkdirSync(generatedRoot, { recursive: true });
  const src = srcDir ?? join(mkdtempSync(join(generatedRoot, 'build-')), 'src');
  const publicDir = join(src, 'public');
  const files = new Map();
  const routes = Object.fromEntries(groups.flatMap(({ pages }) => pages)
    .map(([slug, , source]) => [source, `${base}${slug === 'index' ? '' : `${slug}/`}`]));
  const links = new Map(Object.entries(routes).map(([source, route]) => [join(repository, source), route]));
  for (const [name, source] of Object.entries(downloads)) {
    if (!source.endsWith('SKILL.md')) links.set(join(repository, source), `${base}assets/${name}`);
    files.set(join(publicDir, 'assets', name), sourceBytes(source));
  }
  const titles = {};
  for (const [slug, , source] of groups.flatMap(({ pages }) => pages)) {
    const path = join(repository, source);
    const text = sourceBytes(source).toString('utf8');
    validateMarkdownSource(text, source);
    const front = /^---\r?\n([\s\S]*?)\r?\n---\r?\n/.exec(text);
    let markdown = front ? text.slice(front[0].length) : text;
    const pageFront = front && source.startsWith('docs/site/') ? front[1] : '';
    const title = /^# (.+)\r?$/m.exec(markdown)?.[1] ?? /^title: (.+)\r?$/m.exec(pageFront)?.[1];
    if (!title) throw new Error(`Public page has no title: ${source}`);
    if (!/^search: false\r?$/m.test(pageFront)) titles[routes[source]] = title;
    markdown = markdown.replace(/\]\(([^)]+)\)/g, (match, reference) => {
      if (/^(?:[a-z]+:|#|\/)/i.test(reference)) return match;
      const [filename, suffix = ''] = reference.split(/(?=[?#])/s);
      const target = resolve(dirname(path), filename);
      const route = suffix === '?raw=1' && target === join(repository, '.agents/skills/parcela/SKILL.md')
        ? `${base}assets/parcela-skill.txt` : links.get(target);
      if (!route) throw new Error(`Undeclared public link: ${reference} from ${source}`);
      return `](${route.slice(base.length - 1)}${suffix === '?raw=1' ? '' : suffix})`;
    });
    const output = join(src, slug === 'index' ? 'index.md' : `${slug}/index.md`);
    const frontmatter = /^title: /m.test(pageFront) ? pageFront : `title: ${JSON.stringify(title)}\n${pageFront}`;
    files.set(output, Buffer.from(`---\n${frontmatter.trimEnd()}\n---\n\n${markdown}`));
  }
  files.set(join(publicDir, 'documentation-pages.json'), Buffer.from(JSON.stringify(titles, null, 2) + '\n'));
  for (const [output, bytes] of files) {
    if (existsSync(output) && readFileSync(output).equals(bytes)) continue;
    mkdirSync(dirname(output), { recursive: true });
    writeFileSync(output, bytes);
  }
  return { srcDir: src };
}

export function contentWatchPlugin(base, srcDir) {
  return {
    name: 'parcela-canonical-docs',
    apply: 'serve',
    configureServer(server) {
      const sources = new Set([
        ...groups.flatMap(({ pages }) => pages.map(([, , source]) => join(repository, source))),
        ...Object.values(downloads).map((source) => join(repository, source)),
      ]);
      const publicRoot = join(repository, 'docs/site');
      let lastError;
      function refresh() {
        try {
          prepareContent(base, srcDir);
          if (lastError) server.ws.send({ type: 'full-reload' });
          lastError = undefined;
        } catch (error) {
          lastError = error;
          server.config.logger.error(error.message);
          server.ws.send({ type: 'error', err: { message: error.message, stack: error.stack ?? '' } });
        }
      }
      const directories = new Set([...sources].map(dirname));
      const watchers = [...directories].map((directory) => watch(directory, { recursive: directory === publicRoot }, (_event, filename) => {
        const path = filename && resolve(directory, filename.toString());
        if (!path || sources.has(path) || path === publicRoot || path.startsWith(publicRoot + sep)) refresh();
      }));
      for (const watcher of watchers) watcher.on('error', (error) => {
        lastError = error;
        server.config.logger.error(error.message);
        server.ws.send({ type: 'error', err: { message: error.message, stack: error.stack ?? '' } });
      });
      server.httpServer?.once('close', () => watchers.forEach((watcher) => watcher.close()));
    },
  };
}
