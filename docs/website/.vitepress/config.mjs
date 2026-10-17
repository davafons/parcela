import { defineConfig } from 'vitepress';
import { slug } from 'github-slugger';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { contentWatchPlugin, groups, prepareContent } from '../content.mjs';

const project = fileURLToPath(new URL('..', import.meta.url));
const base = process.env.DOCS_BASE ?? '/parcela/';
if (!/^\/(?:[a-zA-Z0-9_-]+\/)*$/.test(base)) {
  throw new Error('DOCS_BASE must be / or a slash-delimited project path such as /parcela/');
}
const site = process.env.DOCS_SITE ?? 'https://example.invalid';
const origin = new URL(site);
if (origin.protocol !== 'https:' || origin.pathname !== '/' || origin.search || origin.hash || origin.username || origin.password) {
  throw new Error('DOCS_SITE must be an HTTPS origin without a path, query or credentials');
}

const content = prepareContent(base);

export default defineConfig({
  ...content,
  vite: { plugins: [contentWatchPlugin(base, content.srcDir)] },
  base,
  outDir: resolve(project, process.env.DOCS_OUTPUT ?? '../../target/docs'),
  cacheDir: resolve(project, '.generated/cache'),
  cleanUrls: true,
  title: 'Parcela',
  description: 'Independent development environments for your agents and worktrees.',
  head: [['link', { rel: 'icon', href: `${base}assets/logo.svg?v=picket-a` }]],
  transformHead({ pageData }) {
    if (pageData.relativePath === '404.md') return [];
    const route = pageData.relativePath.replace(/index\.md$/, '');
    return [['link', { rel: 'canonical', href: `${origin.origin}${base}${route}` }]];
  },
  transformHtml(html, _id, { page, content }) {
    if (page !== '404.md') return html;
    return html.replace('<div id="app"></div>', `<div id="app">${content}</div>`);
  },
  markdown: {
    html: false,
    attrs: { disable: true },
    anchor: { slugify: slug },
    config(markdown) {
      markdown.block.ruler.disable('snippet');
      const render = markdown.renderer.render.bind(markdown.renderer);
      markdown.renderer.render = (...args) => `<div v-pre>\n${render(...args)}\n</div>`;
    },
  },
  themeConfig: {
    logo: '/assets/logo.svg',
    outline: [2, 3],
    search: { provider: 'local' },
    sidebar: groups.map(({ label, pages }) => ({
      text: label,
      items: pages.map(([slug, text]) => ({ text, link: slug === 'index' ? '/' : `/${slug}/` })),
    })),
    socialLinks: [{ icon: 'github', link: 'https://github.com/davafons/parcela' }],
  },
});
