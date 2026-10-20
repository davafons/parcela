<script setup>
import { onBeforeUnmount, onMounted, ref } from 'vue';

// Land plots behind the home hero, laid out from the page itself: fences fall on the content
// edges and the gap between the demo columns, and stop short of every text block and widget.
const root = ref(null);
const field = ref(null);
let observer;
let frame = 0;

const pad = 22;
const feather = 12;

// The text of the headline, without the rotator's hidden names (they are wider than the shown one).
function headline() {
  const heading = document.querySelector('.VPHero .heading');
  if (!heading) return [];
  const walker = document.createTreeWalker(heading, NodeFilter.SHOW_TEXT);
  const found = [];
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!node.textContent.trim() || node.parentElement.closest('.rotator, .sr')) continue;
    const range = document.createRange();
    range.selectNodeContents(node);
    found.push(...range.getClientRects());
  }
  return found;
}

function rects(selector, tight) {
  return [...document.querySelectorAll(selector)].flatMap((element) => {
    if (!tight) return [element.getBoundingClientRect()];
    const range = document.createRange();
    range.selectNodeContents(element);
    return [...range.getClientRects()];
  });
}

function measure() {
  cancelAnimationFrame(frame);
  frame = requestAnimationFrame(() => {
    const box = root.value?.getBoundingClientRect();
    const terminal = document.querySelector('.demo .terminal')?.getBoundingClientRect();
    const grid = document.querySelector('.demo .zone')?.getBoundingClientRect();
    if (!box || !terminal || !grid) return;
    const left = terminal.left - box.left;
    const right = Math.max(terminal.right, grid.right) - box.left;
    const across = right - left > 700 ? 8 : 4;
    const size = (right - left) / across;
    const top = terminal.top - box.top;
    // The field runs on under the landing sections and fades out where the Markdown content,
    // starting with Install, begins.
    const next = document.querySelector('.VPHome .vp-doc h2')?.getBoundingClientRect();
    const end = next ? next.top - 24 : Math.max(terminal.bottom, grid.bottom) + 72;
    const height = Math.round(end - box.top);
    const xs = [];
    for (let x = left - Math.ceil(left / size) * size; x <= box.width + 1; x += size) xs.push(x);
    const ys = [];
    for (let y = top - Math.ceil(top / size) * size; y <= height + 1; y += size) ys.push(y);
    // Every text block and widget, from the hero down, keeps the same clear margin. In the sharing
    // diagram only the hub, devices and wire labels clear the field, so fences run between them.
    const holes = [
      ...headline(),
      ...rects('.VPHero .heading .rotator', false),
      ...rects('.VPHero .tagline', true),
      ...rects('.VPHero .install, .VPHero .agent-install a', false),
      ...rects('.VPHero .actions .action', false),
      terminal,
      grid,
      ...rects('.why h2, .share h2', true),
      ...rects('.why .lead, .share .lead', true),
      ...rects('.why .panel, .share .hub, .share .dev, .share .wire code', false),
    ].filter((rect) => rect.width && rect.height).map((rect) => ({
      x: rect.left - box.left - pad,
      y: rect.top - box.top - pad,
      w: rect.width + pad * 2,
      h: rect.height + pad * 2,
    }));
    field.value = { width: box.width, height, xs, ys, size, left, top, holes };
  });
}

onMounted(() => {
  measure();
  observer = new ResizeObserver(measure);
  observer.observe(document.body);
  const rotator = document.querySelector('.VPHero .heading .rotator');
  if (rotator) observer.observe(rotator);
  document.fonts?.ready.then(measure);
});

onBeforeUnmount(() => {
  observer?.disconnect();
  cancelAnimationFrame(frame);
});
</script>

<template>
  <div ref="root" class="background" :style="field ? { height: `${field.height}px` } : null" aria-hidden="true">
    <svg v-if="field" :width="field.width" :height="field.height">
      <defs>
        <!-- Fences and dots fade out around each hole instead of stopping at a hard edge. -->
        <filter id="parcela-plots-feather" x="-10%" y="-10%" width="120%" height="120%">
          <feGaussianBlur :stdDeviation="feather" />
        </filter>
        <mask id="parcela-plots-mask" maskUnits="userSpaceOnUse" x="0" y="0" :width="field.width" :height="field.height">
          <rect :width="field.width" :height="field.height" fill="#fff" />
          <g filter="url(#parcela-plots-feather)">
            <rect v-for="(hole, i) in field.holes" :key="i" :x="hole.x" :y="hole.y" :width="hole.w" :height="hole.h" rx="18" fill="#000" />
          </g>
        </mask>
        <pattern
          id="parcela-plots-dots"
          patternUnits="userSpaceOnUse"
          :width="field.size / 8"
          :height="field.size / 8"
          :x="field.left + field.size / 16"
          :y="field.top + field.size / 16"
        >
          <circle :cx="field.size / 16" :cy="field.size / 16" r="1" class="dot" />
        </pattern>
      </defs>
      <g mask="url(#parcela-plots-mask)">
        <rect :width="field.width" :height="field.height" fill="url(#parcela-plots-dots)" />
        <line v-for="x in field.xs" :key="`x${x}`" :x1="x" :x2="x" y1="0" :y2="field.height" class="fence" />
        <line v-for="y in field.ys" :key="`y${y}`" x1="0" :x2="field.width" :y1="y" :y2="y" class="fence" />
      </g>
    </svg>
  </div>
</template>

<style scoped>
/* Faint on purpose: it frames the hero without competing with it. */
.background {
  position: absolute;
  z-index: -1;
  top: 0;
  left: 0;
  right: 0;
  height: 1300px;
  overflow: hidden;
  pointer-events: none;
  opacity: 0.6;
  /* It starts right under the nav bar, so only its bottom edge fades. */
  -webkit-mask-image: linear-gradient(#000 calc(100% - 160px), transparent);
  mask-image: linear-gradient(#000 calc(100% - 160px), transparent);
}

svg {
  display: block;
}

.fence {
  stroke: var(--parcela-bg-mark);
  stroke-width: 1;
  shape-rendering: crispEdges;
}

.dot {
  fill: var(--parcela-bg-mark);
}
</style>
