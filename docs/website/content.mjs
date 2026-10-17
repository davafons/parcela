import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, watch, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const project = dirname(fileURLToPath(import.meta.url));
const repository = resolve(project, '../..');

export const groups = [
  { label: 'Learn', pages: [
    ['index', 'Overview', 'docs/site/index.md'],
    ['install', 'Build and host setup', 'docs/site/install.md'],
    ['first-environment', 'First environment', 'docs/site/first-environment.md'],
    ['worktrees', 'Manage worktrees', 'docs/site/worktrees.md'],
    ['agents', 'Work with an agent', 'docs/site/agents.md'],
  ] },
  { label: 'Guides', pages: [
    ['services', 'Configure service sets', 'docs/site/services.md'],
    ['troubleshooting', 'Troubleshoot and recover', 'docs/site/troubleshooting.md'],
  ] },
  { label: 'Reference', pages: [
    ['configuration', 'Configuration and CLI', 'docs/configuration.md'],
    ['operating-skill', 'Operating skill', '.agents/skills/parcela/SKILL.md'],
    ['limits', 'Supported scope', 'docs/site/limits.md'],
  ] },
];

export const downloads = {
  'parcela-skill.txt': '.agents/skills/parcela/SKILL.md',
  'logo.svg': 'logo.svg',
  'third-party-notices.txt': 'docs/website/THIRD_PARTY_NOTICES.txt',
  'demo-parcela.yaml': 'examples/docs-demo/parcela.yaml',
  'demo-frontend.html': 'examples/docs-demo/frontend/index.html',
  'demo-backend.json': 'examples/docs-demo/backend/status.json',
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
    let markdown = sourceBytes(source).toString('utf8').replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, '');
    validateMarkdownSource(markdown, source);
    const heading = /^# (.+)\r?$/m.exec(markdown);
    if (!heading) throw new Error(`Public page has no title: ${source}`);
    const title = heading[1];
    titles[routes[source]] = title;
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
    files.set(output, Buffer.from(`---\ntitle: ${JSON.stringify(title)}\n---\n\n${markdown}`));
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
