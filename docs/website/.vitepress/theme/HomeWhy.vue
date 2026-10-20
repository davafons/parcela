<script setup>
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { withBase } from 'vitepress';

const players = ['#34bfed', '#ff5a5f', '#3cc46a'];
const worktrees = [
  { path: 'shop/fix-cart-total', url: 'https://fix-cart-total-shop.localhost' },
  { path: 'shop/confirm-email', url: 'https://confirm-email-shop.localhost' },
  { path: 'shop/fuzzy-search', url: 'https://fuzzy-search-shop.localhost' },
];
const net = ref(null);
const hub = ref(null);
const ends = { local: ref(null), lan: ref(null), tailnet: ref(null) };
const wires = ref([]);
// Packets run along the wires only when motion is welcome; CSS animations stop on their own.
const moving = ref(false);
const compare = ref(null);
const armed = ref(false);
const played = ref(false);
let resize;
let seen;

// Positions come from layout boxes, not the screen: the devices rise into place as they scroll
// in, and measuring them mid-rise would leave the arrows ending inside them.
function measure() {
  if (!net.value) return;
  const box = { width: net.value.offsetWidth };
  const center = (el, edge) => {
    const top = el.offsetTop;
    const height = el.offsetHeight;
    return [el.offsetLeft + el.offsetWidth / 2, edge === 'bottom' ? top + height : edge === 'top' ? top : top + height / 2];
  };
  // Narrow, the labels sit closer to their devices so they do not meet.
  const t = box.width < 700 ? 0.68 : 0.5;
  // Each arrow leaves the middle of the proxy and stops just above its device. Its label sits
  // on the arrow itself, partway down the open stretch below the proxy.
  const [cx, cy] = center(hub.value);
  const [, hy] = center(hub.value, 'bottom');
  wires.value = Object.entries(ends).map(([name, el]) => {
    const [x, top] = center(el.value, 'top');
    const y = top - 14;
    const my = hy + (y - hy) * t;
    const mx = cx + (x - cx) * ((my - cy) / (y - cy));
    return { name, x1: cx, y1: cy, x2: x, y2: y, mx, my };
  });
}

onMounted(() => {
  moving.value = !window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (moving.value) {
    armed.value = true;
    seen = new IntersectionObserver(([entry]) => {
      if (!entry.isIntersecting) return;
      played.value = true;
      seen.disconnect();
    }, { threshold: 0.4 });
    seen.observe(compare.value);
  }
  // The devices settle after fonts and images load, so each one is watched too.
  resize = new ResizeObserver(measure);
  for (const el of [net.value, hub.value, ...Object.values(ends).map((end) => end.value)]) resize.observe(el);
  measure();
});

onBeforeUnmount(() => {
  resize?.disconnect();
  seen?.disconnect();
});

const labels = {
  local: ['parcela ', 'up'],
  lan: ['parcela up ', '--lan'],
  tailnet: ['parcela up ', '--tailscale'],
};
</script>

<template>
  <section class="why">
    <div class="inner">
      <h2>Let every agent <em>verify its own work</em></h2>
      <p class="lead">Agents that share one dev server quickly become the bottleneck. Give each one its own environment to run, check and verify what it built.</p>

      <div ref="compare" class="compare" :class="{ armed, played }">
        <div class="panel before">
          <h3><span class="sign">✗</span>Without Parcela</h3>
          <div v-for="(tree, i) in worktrees" :key="tree.path" class="row" :style="{ '--color': players[i], '--i': i }">
            <span class="player">{{ i + 1 }}</span>
            <code><span class="dim">{{ tree.path }} $</span> bin/dev</code>
            <code v-if="i === 0" class="out"><span class="ok">✓</span> <span class="url">Listening on localhost:3000</span></code>
            <code v-else class="out bad">Port 3000 is already in use</code>
          </div>
        </div>
        <div class="panel after">
          <h3><span class="sign">✓</span>With Parcela</h3>
          <div v-for="(tree, i) in worktrees" :key="tree.path" class="row" :style="{ '--color': players[i], '--i': i + 3.6 }">
            <span class="player">{{ i + 1 }}</span>
            <code><span class="dim">{{ tree.path }} $</span> parcela up</code>
            <code class="out"><span class="ok">✓</span> <span class="url">{{ tree.url }}</span></code>
          </div>
        </div>
      </div>

    </div>
  </section>

  <section class="share">
    <div class="inner">
      <h2>Open it <em>from everywhere</em></h2>
      <p class="lead">Every worktree gets its own URL on your machine. Add <code>--lan</code> to open it on your phone, or <code>--tailscale</code> on any device in your tailnet.</p>

      <div ref="net" class="net">
        <svg class="lines" aria-hidden="true">
          <defs>
            <marker id="parcela-arrow" viewBox="0 0 10 10" refX="7" refY="5" markerWidth="9" markerHeight="9" orient="auto-start-reverse">
              <path d="M1 1 L8 5 L1 9" class="head" />
            </marker>
          </defs>
          <line v-for="w in wires" :key="w.name" :x1="w.x1" :y1="w.y1" :x2="w.x2" :y2="w.y2" marker-end="url(#parcela-arrow)" />
          <template v-if="moving">
            <circle v-for="(w, i) in wires" :key="`packet-${w.name}`" r="4.5" class="packet">
              <animateMotion :path="`M${w.x1} ${w.y1} L${w.x2} ${w.y2}`" dur="1.6s" :begin="`${i * 0.9}s`" repeatCount="indefinite" />
              <animate attributeName="opacity" values="0;1;1;0" keyTimes="0;0.2;0.8;1" dur="1.6s" :begin="`${i * 0.9}s`" repeatCount="indefinite" />
            </circle>
          </template>
        </svg>
        <figure :ref="(el) => (ends.local.value = el)" class="dev laptop local">
          <div class="lid">
            <div class="browser">
              <div class="page"><b>Sign up</b><i class="field"></i><i class="field"></i><i class="go"></i><i class="note"></i><i class="note short"></i></div>
            </div>
          </div>
          <div class="base"></div>
          <figcaption><strong>Your machine</strong><span class="flag">up</span><code>https://confirm-email-shop.localhost</code></figcaption>
        </figure>
        <div ref="hub" class="hub">
          <img :src="withBase('/assets/logo.svg')" alt="" width="36" height="36" />
          <strong>parcela</strong>
        </div>
        <figure :ref="(el) => (ends.lan.value = el)" class="dev phone">
          <div class="handset">
            <div class="browser">
              <div class="page"><b>Sign up</b><i class="field"></i><i class="field"></i><i class="go"></i><i class="note"></i><i class="note short"></i></div>
            </div>
          </div>
          <figcaption><strong>Your phone</strong><span class="flag">--lan</span><code>https://confirm-email-shop.local</code></figcaption>
        </figure>
        <figure :ref="(el) => (ends.tailnet.value = el)" class="dev laptop tailnet">
          <div class="lid">
            <div class="browser">
              <div class="page"><b>Sign up</b><i class="field"></i><i class="field"></i><i class="go"></i><i class="note"></i><i class="note short"></i></div>
            </div>
          </div>
          <div class="base"></div>
          <figcaption><strong>Another computer</strong><span class="flag">--tailscale</span><code>https://mb.tail1234.ts.net</code></figcaption>
        </figure>
        <div v-for="w in wires" :key="w.name" class="wire" :style="{ left: `${w.mx}px`, top: `${w.my}px` }">
          <code><span class="up">{{ labels[w.name][0] }}</span>{{ labels[w.name][1] }}</code>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.why {
  margin-top: 96px;
}

.inner {
  box-sizing: border-box;
  margin: 0 auto;
  padding: 0 24px;
  max-width: 1280px;
}

@media (min-width: 640px) {
  .inner {
    padding: 0 48px;
  }
}

@media (min-width: 960px) {
  .inner {
    padding: 0 64px;
  }
}

h2 {
  margin: 0;
  font-size: 30px;
  font-weight: 800;
  line-height: 1.2;
  letter-spacing: -0.01em;
  text-align: center;
  text-wrap: balance;
}

@media (min-width: 960px) {
  h2 {
    font-size: 40px;
  }
}

/* The part of a heading that matters, in the hero's blue. */
h2 em {
  background: var(--parcela-hero);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
  font-style: normal;
}

h2 code {
  color: var(--vp-c-brand-1);
  font-family: var(--vp-font-family-mono);
  font-size: 0.9em;
}

.lead {
  margin: 14px auto 0;
  max-width: 840px;
  color: var(--vp-c-text-2);
  font-size: 18px;
  line-height: 1.55;
  text-align: center;
  text-wrap: balance;
}

code {
  font-family: var(--vp-font-family-mono);
}

.compare {
  display: grid;
  gap: 16px;
  margin-top: 40px;
}

@media (min-width: 960px) {
  .compare {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 32px;
  }
}

.panel {
  border: 1px solid var(--parcela-frame);
  border-radius: 22px;
  background: var(--vp-c-bg);
  box-shadow: var(--parcela-card);
  display: grid;
  gap: 10px;
  padding: 20px;
}

.panel h3 {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 0 4px;
  font-size: 16px;
  font-weight: 800;
}

.sign {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  color: #fff;
  font-size: 12px;
}

.before .sign {
  background: #ff5a5f;
}

.after {
  border-color: var(--parcela-blue);
}

.after .sign {
  background: #2fb35a;
}

.row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  column-gap: 12px;
  row-gap: 2px;
  padding: 10px 14px 10px 12px;
  border-radius: 14px;
  background: var(--parcela-slot);
  font-size: 12px;
  line-height: 1.6;
}

.row code {
  grid-column: 2;
  overflow-wrap: anywhere;
}

/* The player badge is centered on the whole row, beside both lines. */
.player {
  grid-row: span 2;
  align-self: center;
  display: grid;
  place-items: center;
  width: 24px;
  height: 32px;
  border-radius: 8px;
  background: var(--color);
  color: #fff;
  font-size: 11px;
  font-weight: 800;
}

.dim {
  color: var(--vp-c-text-3);
}

.ok {
  color: #1f8a45;
  font-style: normal;
  font-weight: 700;
}

.bad {
  color: #d93a3f;
  font-weight: 600;
}

/* A worktree that came up shows its URL in the same green as its check. */
.url {
  color: #1f8a45;
  font-weight: 600;
}

:root.dark .url {
  color: #5fd68a;
}

:root.dark .ok {
  color: #5fd68a;
}

:root.dark .bad {
  color: #ff8a8d;
}

/* The comparison's playback. Until it scrolls into view, the rows wait hidden; then each
   row's command appears, its output types out a beat later, and a clash shakes once. */
.compare.armed .row,
.compare.armed .out,
.compare.armed .sign {
  opacity: 0;
}

.compare.played .row {
  --at: calc(var(--i) * 0.45s);
  animation: row-in 0.35s ease-out var(--at) both;
}

.compare.played .out {
  animation: type 0.5s steps(24, end) calc(var(--at) + 0.3s) both;
}

.compare.played .out.bad {
  animation:
    type 0.5s steps(24, end) calc(var(--at) + 0.3s) both,
    shake 0.4s ease-in-out calc(var(--at) + 0.85s);
}

.compare.played .out .ok {
  display: inline-block;
  animation: pop 0.35s cubic-bezier(0.34, 1.56, 0.64, 1) calc(var(--at) + 0.3s) both;
}

.compare.played .sign {
  animation: pop 0.35s cubic-bezier(0.34, 1.56, 0.64, 1) both;
}

.compare.played .after .sign {
  animation-delay: 1.5s;
}

@keyframes row-in {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: none; }
}

@keyframes type {
  from { opacity: 1; clip-path: inset(0 100% 0 0); }
  to { opacity: 1; clip-path: inset(0 0 0 0); }
}

@keyframes shake {
  20%, 60% { transform: translateX(-4px); }
  40%, 80% { transform: translateX(4px); }
}

@keyframes pop {
  from { opacity: 0; transform: scale(0.4); }
  to { opacity: 1; transform: none; }
}

/* The sharing diagram: the machine on the left, a wire to each device on the right. */
.share {
  margin-top: 112px;
}

.lead code {
  padding: 1px 5px;
  white-space: nowrap;
  border-radius: 6px;
  background: var(--vp-code-bg);
  color: var(--vp-code-color);
  font-size: 0.86em;
}

/* The proxy on top, an arrow down to each way in: this machine, your phone, another computer.
   The devices stand on one line, so their captions line up. */
.net {
  position: relative;
  z-index: 0;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  grid-template-areas: 'hub hub hub' 'local lan tailnet';
  column-gap: 12px;
  row-gap: 56px;
  justify-items: center;
  align-items: end;
  margin-top: 40px;
}

.local { grid-area: local; }
.hub { grid-area: hub; }
.phone { grid-area: lan; }
.tailnet { grid-area: tailnet; }

@media (min-width: 760px) {
  .net {
    column-gap: 32px;
    row-gap: 120px;
  }

  figcaption code {
    white-space: nowrap;
  }
}


.lines {
  position: absolute;
  z-index: -1;
  inset: 0;
  width: 100%;
  height: 100%;
  overflow: visible;
}

.lines .head {
  fill: none;
  stroke: var(--parcela-blue);
  stroke-width: 1.6;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.lines line {
  stroke: var(--parcela-blue);
  stroke-width: 3;
  stroke-dasharray: 8 8;
  stroke-linecap: round;
  animation: flow 0.9s linear infinite;
}

/* The dashes march from the proxy out to each device; a packet rides along. */
@keyframes flow {
  to { stroke-dashoffset: -16; }
}

.packet {
  fill: var(--parcela-blue);
  opacity: 0;
}

.hub {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 20px 12px 14px;
  border: 1px solid var(--parcela-blue);
  border-radius: 999px;
  background: var(--vp-c-bg);
  box-shadow: var(--parcela-glow);
  animation: breathe 3s ease-in-out infinite;
}

.hub strong {
  font-size: 20px;
  font-weight: 800;
}

.dev {
  display: flex;
  flex-direction: column;
  align-items: center;
  margin: 0;
  width: 100%;
}

.laptop {
  max-width: min(340px, 86%);
}

/* The devices, drawn in the page's own frame color, each showing the signup page. */
.lid {
  box-sizing: border-box;
  width: 100%;
  padding: 8px;
  border: 1px solid var(--parcela-frame);
  border-radius: 16px 16px 4px 4px;
  background: var(--vp-c-bg);
  box-shadow: var(--parcela-card);
}

.base {
  width: 116%;
  height: 12px;
  border: 1px solid var(--parcela-frame);
  border-radius: 2px 2px 12px 12px;
  background: var(--parcela-slot);
}

.handset {
  box-sizing: border-box;
  width: 100%;
  max-width: 104px;
  padding: 5px;
  border: 1px solid var(--parcela-frame);
  border-radius: 26px;
  background: var(--vp-c-bg);
  box-shadow: var(--parcela-card);
}

.browser {
  overflow: hidden;
  border-radius: 6px;
  background: var(--parcela-slot);
}

/* Screens in their real shapes: a MacBook's 16:10 and an iPhone's 19.5:9. */
.lid .browser {
  aspect-ratio: 16 / 10;
}

.handset .browser {
  position: relative;
  aspect-ratio: 9 / 19.5;
  border-radius: 20px;
}

/* The Dynamic Island. */
.handset .browser::before {
  content: '';
  position: absolute;
  top: 7px;
  left: 50%;
  width: 34%;
  height: 10px;
  border-radius: 999px;
  background: var(--parcela-frame);
  transform: translateX(-50%);
}

.page {
  display: flex;
  flex-direction: column;
  gap: 7px;
  padding: 14px 16px 18px;
}

.page b {
  font-size: 13px;
  font-weight: 800;
}

.page i {
  display: block;
  height: 14px;
  border-radius: 999px;
}

.page .field {
  border: 1px solid var(--parcela-frame);
  background: var(--vp-c-bg);
}

.page .go {
  margin-top: 3px;
  background: #ff5a5f;
}

.page .note {
  height: 6px;
  margin-top: 4px;
  width: 80%;
  background: var(--parcela-frame);
  opacity: 0.6;
}

.page .note.short {
  margin-top: 0;
  width: 50%;
}

.phone .page {
  padding: 34px 10px 14px;
}

figcaption {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  margin-top: 14px;
  text-align: center;
}

figcaption strong {
  font-size: 16px;
  font-weight: 800;
}

/* The flag lives on the arrow on wide screens and in the caption on phones. */
figcaption .flag {
  display: none;
}

@media (max-width: 759px) {
  figcaption .flag {
    display: block;
    padding: 1px 8px;
    border: 1px solid var(--parcela-blue);
    border-radius: 999px;
    color: var(--vp-c-brand-1);
    font-family: var(--vp-font-family-mono);
    font-size: 10px;
    font-weight: 700;
    white-space: nowrap;
  }
}

figcaption code {
  padding: 3px 10px;
  border-radius: 999px;
  background: var(--parcela-slot);
  color: var(--vp-c-brand-1);
  font-size: 11px;
  font-weight: 700;
  overflow-wrap: anywhere;
}

@keyframes breathe {
  50% { box-shadow: 0 0 0 7px rgba(52, 191, 237, 0.16), 0 6px 18px rgba(10, 143, 209, 0.18); }
}

/* Each wire's label sits on its midpoint. */
.wire {
  position: absolute;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  font-size: 12px;
  transform: translate(-50%, -50%);
}

.wire code {
  padding: 3px 10px;
  border: 1px solid var(--parcela-blue);
  border-radius: 999px;
  background: var(--vp-c-bg);
  color: var(--vp-c-brand-1);
  font-weight: 700;
  white-space: nowrap;
}

/* On phones the devices shrink to a third of the width: the arrows lose their labels, each
   caption shows its flag in place of its URL, and the small screens tighten their page. */
@media (max-width: 759px) {
  figcaption code,
  .wire {
    display: none;
  }

  .handset {
    max-width: 78px;
    padding: 4px;
    border-radius: 18px;
  }

  .handset .browser {
    border-radius: 14px;
  }

  .handset .browser::before {
    top: 5px;
    height: 6px;
  }

  .phone .page {
    gap: 4px;
    padding: 18px 7px 8px;
  }

  .phone .page b {
    font-size: 9px;
  }

  .phone .page i {
    height: 6px;
  }

  figcaption strong {
    font-size: 13px;
    line-height: 1.3;
  }

  .wire code {
    padding: 2px 8px;
    font-size: 11px;
  }

  .laptop .page {
    gap: 4px;
    padding: 8px 10px;
  }

  .laptop .page b {
    font-size: 10px;
    line-height: 1.3;
  }

  .laptop .page i {
    height: 7px;
  }

  .laptop .page .go {
    margin-top: 1px;
  }

  .laptop .page .note {
    display: none;
  }
}

@supports (animation-timeline: view()) {
  .compare > *,
  .net > figure {
    animation: rise linear both;
    animation-timeline: view();
    animation-range: entry 0% entry 45%;
  }
}

@keyframes rise {
  from {
    opacity: 0;
    transform: translateY(16px);
  }
}
</style>
