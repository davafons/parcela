<script setup>
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { withBase } from 'vitepress';

// The closing call to action above the footer: the install command and the mascot.
// The install block is VitePress's code block markup, so its copy button works the same.
const install = [
  ['#6F42C1', '#B392F0', 'curl'],
  ['#005CC5', '#79B8FF', ' -fsSL'],
  ['#032F62', '#9ECBFF', ' https://parcela.davafons.com/install.sh'],
  ['#D73A49', '#F97583', ' |'],
  ['#6F42C1', '#B392F0', ' sh'],
];
const root = ref(null);
const shown = ref(true);
let observer;

onMounted(() => {
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  shown.value = false;
  observer = new IntersectionObserver(([entry]) => {
    if (!entry.isIntersecting) return;
    shown.value = true;
    observer.disconnect();
  }, { threshold: 0.35 });
  observer.observe(root.value);
});

onBeforeUnmount(() => {
  observer?.disconnect();
});
</script>

<template>
  <section ref="root" class="cta" :class="{ shown }">
    <div class="inner">
      <div class="pitch">
        <h2>Give every agent its own running app</h2>
        <p>Install Parcela, describe your app once, and every worktree runs at its own HTTPS URL.</p>
        <div class="vp-doc install">
          <div class="language-sh vp-adaptive-theme">
            <button title="Copy Code" class="copy"></button>
            <span class="lang">sh</span>
            <pre class="shiki shiki-themes github-light github-dark vp-code" tabindex="0"><code><span class="line"><span v-for="([light, dark, text], i) in install" :key="i" :style="{ '--shiki-light': light, '--shiki-dark': dark }">{{ text }}</span></span></code></pre>
          </div>
        </div>
        <a class="go" :href="withBase('/getting-started/')">Get started</a>
      </div>
      <img class="mascot" :src="withBase('/assets/logo.svg')" alt="" width="200" height="200" aria-hidden="true" />
    </div>
  </section>
</template>

<style scoped>
/* A full-width blue band that runs straight into the footer. */
.cta {
  order: 1;
  margin-top: 112px;
  background: #0877b3;
  color: #ffffff;
}

.inner {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 32px;
  align-items: center;
  box-sizing: border-box;
  margin: 0 auto;
  padding: 64px 24px;
  max-width: 1280px;
}

@media (min-width: 640px) {
  .inner {
    padding: 72px 48px;
  }
}

@media (min-width: 960px) {
  .inner {
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 64px;
    padding: 80px 64px;
  }
}

h2 {
  margin: 0;
  font-size: 32px;
  font-weight: 800;
  line-height: 1.15;
  letter-spacing: -0.01em;
  text-wrap: balance;
}

@media (min-width: 960px) {
  h2 {
    font-size: 44px;
  }
}

p {
  margin: 14px 0 0;
  max-width: 560px;
  color: rgba(255, 255, 255, 0.86);
  font-size: 18px;
  line-height: 1.55;
}

.install {
  margin-top: 24px;
  max-width: 600px;
}

.install div[class*='language-'] {
  margin: 0;
  box-shadow: none;
}

.go {
  display: inline-block;
  margin-top: 24px;
  padding: 0 28px;
  border-radius: 999px;
  background: #ffffff;
  color: #0868a0;
  font-size: 17px;
  font-weight: 700;
  line-height: 48px;
  text-decoration: none;
  transition: box-shadow 0.2s ease;
}

.go:hover {
  box-shadow: 0 0 0 4px rgba(255, 255, 255, 0.3);
}

.mascot {
  justify-self: center;
  width: 200px;
  height: 200px;
  transform-origin: 50% 90%;
}

.shown .mascot {
  animation: hop 0.6s cubic-bezier(0.34, 1.56, 0.64, 1) both, sway 4s ease-in-out 0.8s infinite;
}

@keyframes hop {
  from { opacity: 0; transform: translateY(16px); }
  to { opacity: 1; transform: none; }
}

@keyframes sway {
  0%, 70%, 100% { rotate: 0deg; }
  78% { rotate: -6deg; }
  88% { rotate: 5deg; }
}
</style>
