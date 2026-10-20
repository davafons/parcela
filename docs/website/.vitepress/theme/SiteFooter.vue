<script setup>
import { useData, withBase } from 'vitepress';
import { github } from './agents.js';

const { frontmatter } = useData();
const repo = 'https://github.com/lostcoords/parcela';

// Small line icons for the project links, drawn on a 24px grid.
const icons = {
  github: { fill: github },
  release: { stroke: 'M3 12V4a1 1 0 0 1 1-1h8l9 9-9 9-9-9Z M7.5 7.5h.01' },
  changelog: { stroke: 'M8 6h13 M8 12h13 M8 18h13 M3 6h.01 M3 12h.01 M3 18h.01' },
};

const columns = [
  { title: 'Docs', links: [
    { text: 'Get started', href: withBase('/docs/getting-started/') },
    { text: 'Parallel worktrees', href: withBase('/docs/worktrees/') },
    { text: 'Configuration', href: withBase('/docs/configuration/') },
    { text: 'CLI guide', href: withBase('/docs/cli/') },
  ] },
  { title: 'Help', links: [
    { text: 'FAQ', href: withBase('/docs/faq/') },
    { text: 'Troubleshooting', href: withBase('/docs/troubleshooting/') },
    { text: 'Frameworks and data', href: withBase('/docs/frameworks/') },
    { text: 'Telemetry', href: withBase('/docs/telemetry/') },
  ] },
  { title: 'Project', links: [
    { text: 'GitHub', href: repo, icon: 'github' },
    { text: 'Releases', href: `${repo}/releases`, icon: 'release' },
    { text: 'Changelog', href: `${repo}/blob/main/CHANGELOG.md`, icon: 'changelog' },
  ] },
];
</script>

<template>
  <footer v-if="frontmatter.layout === 'home'" class="footer">
    <div class="inner">
      <nav v-for="column in columns" :key="column.title" class="column" :aria-label="column.title">
        <h3>{{ column.title }}</h3>
        <a v-for="link in column.links" :key="link.text" :href="link.href">
          <svg v-if="link.icon" viewBox="0 0 24 24" aria-hidden="true">
            <path v-if="icons[link.icon].fill" :d="icons[link.icon].fill" fill="currentColor" />
            <path v-else :d="icons[link.icon].stroke" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
          {{ link.text }}
        </a>
      </nav>
    </div>
    <div class="bottom">
      <p>© 2026 LOST COORDS</p>
    </div>
  </footer>
</template>

<style scoped>
.footer {
  border-top: 1px solid var(--vp-c-divider);
  background: var(--vp-c-bg-alt);
  color: var(--vp-c-text-2);
  font-size: 14px;
}

/* The columns and the copyright line share the page's content edges. */
.inner,
.bottom {
  box-sizing: border-box;
  margin: 0 auto;
  padding: 0 24px;
  max-width: 1280px;
}

@media (min-width: 640px) {
  .inner,
  .bottom {
    padding: 0 48px;
  }
}

@media (min-width: 960px) {
  .inner,
  .bottom {
    padding: 0 64px;
  }
}

.inner {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 32px;
  padding-top: 56px;
  padding-bottom: 40px;
}

@media (min-width: 640px) {
  .inner {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

.column {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

h3 {
  margin: 0 0 4px;
  color: var(--vp-c-text-1);
  font-size: 13px;
  font-weight: 800;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.column a {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  color: var(--vp-c-text-2);
  text-decoration: none;
  transition: color 0.2s ease;
}

.column a:hover {
  color: var(--vp-c-brand-1);
}

.column svg {
  flex: none;
  width: 16px;
  height: 16px;
}

.bottom p {
  margin: 0;
  padding: 20px 0 32px;
  border-top: 1px solid var(--vp-c-divider);
  color: var(--vp-c-text-3);
  font-size: 13px;
}
</style>
