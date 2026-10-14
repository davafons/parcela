import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import { groups, prepareContent } from './content.mjs';

const base = process.env.DOCS_BASE ?? '/parcela/';
if (!/^\/(?:[a-zA-Z0-9_-]+\/)*$/.test(base)) {
  throw new Error('DOCS_BASE must be / or a slash-delimited project path such as /parcela/');
}
const site = process.env.DOCS_SITE ?? 'https://example.invalid';
const origin = new URL(site);
if (origin.protocol !== 'https:' || origin.pathname !== '/' || origin.search || origin.hash || origin.username || origin.password) {
  throw new Error('DOCS_SITE must be an HTTPS origin without a path, query or credentials');
}

export default defineConfig({
  ...prepareContent(base),
  site,
  base,
  outDir: process.env.DOCS_OUTPUT ?? '../../target/docs',
  trailingSlash: 'always',
  integrations: [starlight({
    title: 'Parcela',
    favicon: '/assets/logo.svg',
    description: 'Named local HTTPS and native service sets for your development worktrees.',
    logo: { src: '../../logo.svg', alt: '' },
    expressiveCode: {
      styleOverrides: { borderRadius: '0.75rem' },
    },
    social: [{ icon: 'github', label: 'Source on GitHub', href: 'https://github.com/davafons/parcela' }],
    customCss: ['./src/parcela.css'],
    sidebar: groups.map(({ label, pages }) => ({
      label,
      items: pages.map(([slug, title]) => ({ slug: slug === 'index' ? '' : slug, label: title })),
    })),
  })],
});
