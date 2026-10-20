<script setup>
import { useData, withBase } from 'vitepress';
import { github } from './agents.js';

const { frontmatter } = useData();
const repo = 'https://github.com/davafons/parcela';

// Small line icons for the project links, drawn on a 24px grid.
const icons = {
  github: { fill: github },
  release: { stroke: 'M3 12V4a1 1 0 0 1 1-1h8l9 9-9 9-9-9Z M7.5 7.5h.01' },
  changelog: { stroke: 'M8 6h13 M8 12h13 M8 18h13 M3 6h.01 M3 12h.01 M3 18h.01' },
  license: { stroke: 'M12 3v18 M5 21h14 M3 7h18 M6 7l-3 7a3 3 0 0 0 6 0Z M18 7l-3 7a3 3 0 0 0 6 0Z' },
  notices: { stroke: 'M14 3H6a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8Z M14 3v5h5 M9 13h6 M9 17h6' },
};

const columns = [
  { title: 'Docs', links: [
    { text: 'Get started', href: withBase('/getting-started/') },
    { text: 'Parallel worktrees', href: withBase('/worktrees/') },
    { text: 'Configuration', href: withBase('/configuration/') },
    { text: 'CLI guide', href: withBase('/cli/') },
  ] },
  { title: 'Help', links: [
    { text: 'FAQ', href: withBase('/faq/') },
    { text: 'Troubleshooting', href: withBase('/troubleshooting/') },
    { text: 'Frameworks and data', href: withBase('/frameworks/') },
  ] },
  { title: 'Project', links: [
    { text: 'GitHub', href: repo, icon: 'github' },
    { text: 'Releases', href: `${repo}/releases`, icon: 'release' },
    { text: 'Changelog', href: `${repo}/blob/main/CHANGELOG.md`, icon: 'changelog' },
    { text: 'AGPL-3.0 license', href: `${repo}/blob/main/LICENSE`, icon: 'license' },
    { text: 'Third-party notices', href: withBase('/assets/third-party-notices.txt'), icon: 'notices' },
  ] },
];
</script>

<template>
  <footer v-if="frontmatter.layout === 'home'" class="footer">
    <div class="inner">
      <div class="brand">
        <a class="name" :href="withBase('/')">
          <img :src="withBase('/assets/logo.svg')" alt="" width="40" height="40" />
          Parcela
        </a>
        <p>Deployable dev environments for your AI agents.</p>
      </div>
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
      <span>Copyright © 2026 David Afonso. Released under the AGPL-3.0.</span>
      <span class="social">
        <a :href="repo" aria-label="GitHub"><svg viewBox="0 0 24 24" aria-hidden="true"><path :d="icons.github.fill" fill="currentColor" /></svg></a>
        <a :href="`${repo}/releases`" aria-label="Releases"><svg viewBox="0 0 24 24" aria-hidden="true"><path :d="icons.release.stroke" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" /></svg></a>
      </span>
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

@media (min-width: 768px) {
  .inner {
    grid-template-columns: minmax(0, 1.6fr) repeat(3, minmax(0, 1fr));
  }
}

.brand {
  grid-column: 1 / -1;
}

@media (min-width: 768px) {
  .brand {
    grid-column: auto;
  }
}

.name {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  color: var(--vp-c-text-1);
  font-size: 20px;
  font-weight: 800;
  text-decoration: none;
}

.name img {
  width: 40px;
  height: 40px;
}

.brand p {
  margin: 12px 0 0;
  max-width: 280px;
  line-height: 1.55;
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

.column a:hover,
.social a:hover {
  color: var(--vp-c-brand-1);
}

.column svg {
  flex: none;
  width: 16px;
  height: 16px;
}

.bottom {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-top: 20px;
  padding-bottom: 28px;
  border-top: 1px solid var(--vp-c-divider);
  font-size: 13px;
}

@media (min-width: 640px) {
  .bottom {
    border-top: 0;
    background: linear-gradient(var(--vp-c-divider), var(--vp-c-divider)) top / calc(100% - 96px) 1px no-repeat;
  }
}

@media (min-width: 960px) {
  .bottom {
    background-size: calc(100% - 128px) 1px;
  }
}

.social {
  display: flex;
  gap: 8px;
}

.social a {
  display: grid;
  place-items: center;
  width: 36px;
  height: 36px;
  border: 2px solid var(--parcela-frame);
  border-radius: 12px;
  background: var(--vp-c-bg);
  color: var(--vp-c-text-2);
  transition: color 0.2s ease, border-color 0.2s ease;
}

.social a:hover {
  border-color: var(--parcela-blue);
}

.social svg {
  width: 18px;
  height: 18px;
}
</style>
