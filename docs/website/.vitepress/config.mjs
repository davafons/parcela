import { defineConfig } from 'vitepress';
import { slug } from 'github-slugger';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { groups, prepareContent, route, srcDir, watchSources } from '../content.mjs';

const project = fileURLToPath(new URL('..', import.meta.url));
const base = process.env.DOCS_BASE ?? '/docs/';
const site = process.env.DOCS_SITE ?? 'https://parcela.davafons.com';

prepareContent();

export default defineConfig({
  srcDir,
  // `parcela up --lan` or `--tailscale` lists the names other devices use, for Vite's host check.
  vite: {
    plugins: [watchSources],
    server: { allowedHosts: (process.env.PARCELA_SHARE_HOSTS ?? '').split(',').filter(Boolean) },
  },
  base,
  outDir: resolve(project, process.env.DOCS_OUTPUT ?? '../../target/docs'),
  cacheDir: resolve(project, '.generated/cache'),
  cleanUrls: true,
  title: 'Parcela',
  description: 'Deployable dev environments for your AI agents.',
  head: [['link', { rel: 'icon', href: `${base}assets/logo.svg?v=picket-a` }]],
  transformHead({ pageData }) {
    if (pageData.relativePath === '404.md') return [];
    const path = pageData.relativePath.replace(/index\.md$/, '');
    return [['link', { rel: 'canonical', href: `${site}${base}${path}` }]];
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
      items: pages.map(([slug, text]) => ({ text, link: route(slug) })),
    })),
    socialLinks: [{ icon: 'github', link: 'https://github.com/davafons/parcela' }],
  },
});
