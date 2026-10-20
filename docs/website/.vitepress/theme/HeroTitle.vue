<script setup>
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import { useData } from 'vitepress';
import { agents } from './agents.js';

const { frontmatter } = useData();
const current = ref(0);
const width = ref(null);
const items = ref([]);
let timer;

function measure() {
  width.value = items.value[current.value]?.offsetWidth ?? null;
}

onMounted(() => {
  measure();
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  timer = setInterval(async () => {
    current.value = (current.value + 1) % agents.length;
    await nextTick();
    measure();
  }, 2400);
  document.fonts?.ready.then(measure);
});

onBeforeUnmount(() => clearInterval(timer));
</script>

<template>
  <h1 class="heading">
    <span class="text"><span class="line">Deployable dev environments</span> <span class="line">for your <span class="sr">coding agent</span><span
      class="rotator"
      :style="width ? { width: `${width}px` } : null"
      aria-hidden="true"
    ><span
      v-for="(agent, i) in agents"
      :key="agent.name"
      ref="items"
      class="agent"
      :class="{ on: i === current, past: i === (current + agents.length - 1) % agents.length }"
      :style="agent.color ? { color: agent.color } : null"
    ><svg viewBox="0 0 24 24"><path :d="agent.path" fill="currentColor" /></svg><span
      class="name"
      :style="agent.gradient ? { backgroundImage: agent.gradient } : null"
      :class="{ blend: agent.gradient }"
    >{{ agent.name }}</span></span></span></span></span>
  </h1>
  <p class="tagline">{{ frontmatter.hero.tagline }}</p>
</template>

<style scoped>
.line {
  display: block;
}

.sr {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

.rotator {
  position: relative;
  display: inline-block;
  height: 1.12em;
  overflow: hidden;
  vertical-align: bottom;
  text-align: left;
  transition: width 0.45s cubic-bezier(0.45, 0, 0.25, 1);
}

.agent {
  position: absolute;
  left: 0;
  top: 0;
  display: inline-flex;
  align-items: center;
  gap: 0.22em;
  color: var(--vp-c-text-1);
  white-space: nowrap;
  opacity: 0;
  transform: translateY(100%);
  transition: transform 0.45s cubic-bezier(0.45, 0, 0.25, 1), opacity 0.45s ease;
}

/* The first item holds the box open before the script measures. */
.agent:first-child {
  position: relative;
}

.agent.on {
  opacity: 1;
  transform: none;
}

.agent.past {
  transform: translateY(-100%);
}

.blend {
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}

.agent svg {
  flex: none;
  width: 0.78em;
  height: 0.78em;
}
</style>
