<script setup>
import { useRouter, withBase } from 'vitepress';
import guide from '../../../site/getting-started.md?raw';

// A page cannot start a terminal agent, so the link copies the Getting started prompt, which
// installs Parcela, saves its skill and configures the project, then opens it there with its
// copy button showing "Copied!". Without clipboard access it still opens the prompt.
const prompt = /## Install it with your agent\n[\s\S]*?```text\n([\s\S]*?)\n```/.exec(guide)[1];
const href = withBase('/docs/getting-started/#install-it-with-your-agent');
const router = useRouter();

async function open() {
  let copied = true;
  try {
    await navigator.clipboard.writeText(prompt);
  } catch {
    copied = false;
  }
  await router.go(href);
  if (!copied) return;
  const button = document.querySelector('#install-it-with-your-agent ~ div[class*="language-"] > button.copy');
  button?.classList.add('copied');
  setTimeout(() => button?.classList.remove('copied'), 2500);
}
</script>

<template>
  <a class="link" :href="href" @click.prevent="open">
    <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9Z M19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8Z" /></svg>
    Or install it with your AI agent
  </a>
</template>

<style scoped>
.link {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 4px 6px;
  color: var(--vp-c-brand-1);
  font-size: 15px;
  font-weight: 700;
  text-decoration: underline;
  text-decoration-color: transparent;
  text-underline-offset: 4px;
  transition: text-decoration-color 0.2s ease;
}

.link:hover {
  text-decoration-color: currentColor;
}

.link svg {
  width: 16px;
  height: 16px;
  fill: currentColor;
}
</style>
