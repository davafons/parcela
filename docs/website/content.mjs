import { mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const project = dirname(fileURLToPath(import.meta.url));
const repository = resolve(project, '../..');

export const groups = [
  { label: 'Start here', pages: [['index', 'Start here', 'docs/site/index.md']] },
  { label: 'Get started', pages: [
    ['install', 'Build and host setup', 'docs/site/install.md'],
    ['first-environment', 'First environment', 'docs/site/first-environment.md'],
  ] },
  { label: 'Guides', pages: [
    ['worktrees', 'Manage worktrees', 'docs/site/worktrees.md'],
    ['services', 'Configure service sets', 'docs/site/services.md'],
    ['agents', 'Work with an agent', 'docs/site/agents.md'],
    ['troubleshooting', 'Troubleshoot and recover', 'docs/site/troubleshooting.md'],
    ['transfer', 'Paused checkpoints', 'docs/site/transfer.md'],
  ] },
  { label: 'Reference', pages: [
    ['configuration', 'Configuration and CLI', 'docs/configuration.md'],
    ['operating-skill', 'Operating skill', '.agents/skills/parcela/SKILL.md'],
    ['limits', 'Supported scope', 'docs/site/limits.md'],
  ] },
];

const downloads = {
  'parcela-skill.txt': '.agents/skills/parcela/SKILL.md',
  'logo.svg': 'logo.svg',
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

function sourceBytes(source) {
  const path = join(repository, source);
  const resolved = relative(repository, realpathSync(path));
  if (resolved === '..' || resolved.startsWith(`..${sep}`)) {
    throw new Error(`Canonical documentation source escapes the repository: ${source}`);
  }
  return readFileSync(path);
}

export function prepareContent(base) {
  validatePublicSources(join(repository, 'docs/site'));
  const generatedRoot = join(project, '.generated');
  mkdirSync(generatedRoot, { recursive: true });
  const generated = mkdtempSync(join(generatedRoot, 'build-'));
  const src = join(generated, 'src');
  const publicDir = join(generated, 'public');
  mkdirSync(join(src, 'content/docs'), { recursive: true });
  mkdirSync(join(publicDir, 'assets'), { recursive: true });
  const routes = Object.fromEntries(groups.flatMap(({ pages }) => pages)
    .map(([slug, , source]) => [source, `${base}${slug === 'index' ? '' : `${slug}/`}`]));
  const links = new Map(Object.entries(routes).map(([source, route]) => [join(repository, source), route]));
  links.set(join(repository, 'docs/transfer.md'), `${base}transfer/`);
  for (const [name, source] of Object.entries(downloads)) {
    if (!source.endsWith('SKILL.md')) links.set(join(repository, source), `${base}assets/${name}`);
    writeFileSync(join(publicDir, 'assets', name), sourceBytes(source));
  }
  const titles = {};
  for (const [slug, , source] of groups.flatMap(({ pages }) => pages)) {
    const path = join(repository, source);
    let markdown = sourceBytes(source).toString('utf8').replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, '');
    const heading = /^# (.+)\r?$/m.exec(markdown);
    if (!heading) throw new Error(`Public page has no title: ${source}`);
    const title = heading[1];
    titles[routes[source]] = title;
    markdown = markdown.replace(heading[0], '').trimStart();
    markdown = markdown.replace(/\]\(([^)]+)\)/g, (match, reference) => {
      if (/^(?:[a-z]+:|#|\/)/i.test(reference)) return match;
      const [filename, suffix = ''] = reference.split(/(?=[?#])/s);
      const target = resolve(dirname(path), filename);
      const route = suffix === '?raw=1' && target === join(repository, '.agents/skills/parcela/SKILL.md')
        ? `${base}assets/parcela-skill.txt` : links.get(target);
      if (!route) throw new Error(`Undeclared public link: ${reference} from ${source}`);
      return `](${route}${suffix === '?raw=1' ? '' : suffix})`;
    });
    writeFileSync(join(src, 'content/docs', `${slug}.md`), `---\ntitle: ${JSON.stringify(title)}\n---\n\n${markdown}`);
  }
  writeFileSync(join(src, 'content.config.ts'), `import { defineCollection } from 'astro:content';\nimport { docsLoader } from '@astrojs/starlight/loaders';\nimport { docsSchema } from '@astrojs/starlight/schema';\nexport const collections = { docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }) };\n`);
  writeFileSync(join(publicDir, 'documentation-pages.json'), JSON.stringify(titles, null, 2) + '\n');
  return { srcDir: src, publicDir };
}
