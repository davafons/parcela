<script setup>
import { onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { agents } from './agents.js';

// Three agents, one worktree each. An agent opens its terminal tab and its hand appears over
// its slot; the hand keeps working there, `parcela up` puts the worktree in the slot, and
// `parcela down` takes it away and closes the tab.
const envs = [
  { name: 'shop', host: 'shop.localhost', label: 'shop', agent: agents[0] },
  { name: 'signup', host: 'signup-shop.localhost', label: 'signup (shop)', agent: agents[1] },
  { name: 'search', host: 'search-shop.localhost', label: 'search (shop)', agent: agents[2] },
];
const players = ['#34bfed', '#ff5a5f', '#3cc46a'];
const url = (i) => `https://${envs[i].host}:1443`;

// ask: the person's prompt, typed. say: the agent's step. run: a shell command the agent runs,
// with its output. up/down: the run starts or stops that tab's worktree.
const script = [
  [0, 'ask', 'fix rounding in the checkout total'],
  [0, 'say', 'Updated Cart#total to round once, at the end'],
  [0, 'up'],
  [1, 'ask', 'add email confirmation to signup'],
  [1, 'say', 'Added SignupMailer and a /confirm route'],
  [1, 'up'],
  [2, 'ask', 'make product search typo-tolerant'],
  [2, 'say', 'Added a trigram index to products.name'],
  [2, 'up'],
  [0, 'say', `Checked ${url(0)}/cart: $19.99, not $20.00`],
  [0, 'say', 'Tests pass. Ready for review.'],
  [1, 'run', 'parcela logs web --tail 1', [[['dim', 'web | '], ['', 'POST /signup 302, mail queued']]]],
  [1, 'say', `Confirmed a test account on ${url(1)}`],
  [2, 'run', 'parcela status', [[['', 'shop search  '], ['ok', 'ready']], [['url', url(2)]]]],
  [2, 'say', '"shoos" finds Shoes. Tests pass.'],
  [2, 'down'],
  [1, 'down'],
  [0, 'down'],
];

// A hand rests over its own slot and shows a loading ring while its agent works.
const spot = (p) => [(p % 3 + 0.72) / 3 * 100, (Math.floor(p / 3) + 0.4) / 2 * 100];
const pointers = reactive(players.map((_, p) => ({ x: spot(p)[0], y: spot(p)[1], shown: false, working: false, click: 0 })));
// The agent's last step before `down`: after it, the agent is done and its ring stops.
const last = envs.map((_, i) => script.findLastIndex(([tab, kind]) => tab === i && (kind === 'say' || kind === 'run')));

const tabs = reactive(envs.map(() => ({ open: false, busy: false, lines: [] })));
const order = ref([]);
const view = ref(-1);
const focus = ref(-1);
const typed = ref('');
const state = reactive(envs.map(() => 'empty'));
const root = ref(null);
let run = 0;
let held = 0;
let observer;

// Clicking a tab holds it on screen for a while; then the view follows the agents again.
function pick(i) {
  view.value = i;
  held = Date.now() + 8000;
}

function show(i) {
  focus.value = i;
  if (Date.now() > held || !tabs[view.value]?.open) view.value = i;
}

function open(i) {
  tabs[i].open = true;
  order.value = [...order.value, i];
  pointers[i].shown = true;
}

function close(i) {
  tabs[i].open = false;
  tabs[i].busy = false;
  order.value = order.value.filter((tab) => tab !== i);
  pointers[i].shown = false;
  pointers[i].working = false;
  if (view.value === i) view.value = order.value.at(-1) ?? -1;
}

const ask = (text) => [['ask', '› '], ['', text]];
const say = (text) => [['say', '● '], ['', text]];
const shell = (cmd) => [['say', '● '], ['dim', '$ '], ['', cmd]];
const indent = (line, i = 0) => [['dim', i ? '    ' : '  └ '], ...line];
const upLines = (i) => [shell('parcela up --detach'), indent([['ok', '✓'], ['', ` ${envs[i].label} is running at `], ['url', url(i)]])];
const downLines = (i) => [shell('parcela down'), indent([['ok', '✓'], ['', ` Stopped ${envs[i].label}`]])];

function reset(done) {
  tabs.forEach((tab) => Object.assign(tab, { open: false, busy: false, lines: [] }));
  order.value = [];
  view.value = -1;
  focus.value = -1;
  typed.value = '';
  envs.forEach((_, i) => { state[i] = 'empty'; });
  pointers.forEach((pointer) => Object.assign(pointer, { shown: false, working: false }));
  if (!done) return;
  // The resting frame: every agent has its worktree up and its work checked.
  for (const [tab, kind, text, out] of script) {
    if (kind === 'down') break;
    if (!tabs[tab].open) open(tab);
    if (kind === 'up') {
      tabs[tab].lines.push(...upLines(tab));
      state[tab] = 'ready';
    } else if (kind === 'run') {
      tabs[tab].lines.push(shell(text), ...out.map((line, i) => indent(line, i)));
    } else {
      tabs[tab].lines.push(kind === 'ask' ? ask(text) : say(text));
    }
  }
  view.value = 0;
}

async function play(id) {
  const wait = (ms) => new Promise((resolve, reject) => setTimeout(() => (id === run ? resolve() : reject()), ms));
  try {
    for (;;) {
      reset(false);
      await wait(700);
      for (const [step, [tab, kind, text, out]] of script.entries()) {
        if (!tabs[tab].open) {
          open(tab);
          show(tab);
          await wait(500);
        } else if (focus.value !== tab) {
          show(tab);
          await wait(450);
        }
        const lines = tabs[tab].lines;
        if (kind === 'ask') {
          for (const char of text) {
            typed.value += char;
            await wait(30);
          }
          await wait(300);
          lines.push(ask(text));
          typed.value = '';
          tabs[tab].busy = true;
          pointers[tab].working = true;
          await wait(900);
        } else if (kind === 'say') {
          pointers[tab].click += 1;
          lines.push(say(text));
          await wait(1100);
        } else if (kind === 'run') {
          pointers[tab].click += 1;
          lines.push(shell(text));
          await wait(600);
          lines.push(...out.map((line, i) => indent(line, i)));
          await wait(1200);
        } else {
          const [command, result] = kind === 'up' ? upLines(tab) : downLines(tab);
          lines.push(command);
          pointers[tab].click += 1;
          state[tab] = kind === 'up' ? 'starting' : 'stopping';
          await wait(1100);
          state[tab] = kind === 'up' ? 'ready' : 'empty';
          lines.push(result);
          await wait(kind === 'up' ? 900 : 1000);
          if (kind === 'down') close(tab);
        }
        if (step === last[tab]) pointers[tab].working = false;
      }
      await wait(1800);
    }
  } catch {
    // A newer run took over.
  }
}

onMounted(() => {
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  observer = new IntersectionObserver(([entry]) => {
    run += 1;
    if (entry.isIntersecting) play(run);
    else reset(true);
  });
  observer.observe(root.value);
});

onBeforeUnmount(() => {
  run += 1;
  observer?.disconnect();
});

reset(true);
</script>

<template>
  <section ref="root" class="demo" aria-label="Three coding agents, each in its own worktree, start and stop their environments with parcela up and parcela down">
    <div class="terminal">
      <div class="tabs" role="tablist" aria-label="Agents">
        <button
          v-for="i in order"
          :key="envs[i].name"
          type="button"
          role="tab"
          class="tab"
          :class="{ on: i === view }"
          :aria-selected="i === view"
          :style="{ '--color': players[i] }"
          @click="pick(i)"
        >
          <span class="player">{{ i + 1 }}</span>
          <svg viewBox="0 0 24 24" aria-hidden="true" :style="envs[i].agent.color ? { color: envs[i].agent.color } : null"><path :d="envs[i].agent.path" fill="currentColor" /></svg>
          <span class="tab-name">{{ envs[i].name }}</span>
          <span v-if="tabs[i].busy && i !== view" class="busy"></span>
        </button>
      </div>
      <div class="screen" aria-hidden="true">
        <pre v-if="view >= 0" :style="{ '--color': players[view] }"><code><span v-for="(line, i) in tabs[view].lines" :key="`${view}-${i}`" class="line"><span v-for="([kind, text], j) in line" :key="j" :class="kind">{{ text }}</span></span><span class="line"><span class="ask">› </span>{{ view === focus ? typed : '' }}<span class="cursor"></span></span></code></pre>
      </div>
    </div>
    <div class="agents" aria-hidden="true">
      <div class="grid">
        <div v-for="slot in 6" :key="slot" class="slot">
          <div v-if="slot <= envs.length && state[slot - 1] !== 'empty'" class="tile" :class="state[slot - 1]">
            <div class="head">
              <strong>{{ envs[slot - 1].name }}</strong>
              <span class="pill" :class="state[slot - 1]"><span class="mark"></span>{{ state[slot - 1] }}</span>
            </div>
            <span class="host">{{ envs[slot - 1].host }}</span>
          </div>
        </div>
        <div
          v-for="(pointer, p) in pointers"
          :key="p"
          class="pointer"
          :class="{ shown: pointer.shown }"
          :style="{ left: `${pointer.x}%`, top: `${pointer.y}%`, '--color': players[p] }"
        >
          <span v-if="pointer.click" :key="pointer.click" class="ring"></span>
          <span v-if="pointer.working" class="loading"><i v-for="dot in 8" :key="dot" :style="{ '--dot': dot - 1 }"></i></span>
          <svg :key="`hand-${pointer.click}`" viewBox="-20 -6 64 86" class="hand">
            <g transform="translate(5 7)" opacity="0.16" stroke="#000" stroke-width="10" stroke-linejoin="round">
              <rect x="-8" y="0" width="16" height="48" rx="8" /><rect x="-12" y="30" width="46" height="42" rx="16" />
            </g>
            <g :fill="players[p]" :stroke="players[p]" stroke-width="9" stroke-linejoin="round">
              <rect x="-8" y="0" width="16" height="48" rx="8" /><rect x="-12" y="30" width="46" height="42" rx="16" />
            </g>
            <g fill="#fff"><rect x="-8" y="0" width="16" height="48" rx="8" /><rect x="-12" y="30" width="46" height="42" rx="16" /></g>
            <text x="11" y="61" text-anchor="middle" font-size="25" font-weight="800" :fill="players[p]">{{ p + 1 }}</text>
          </svg>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.demo {
  display: grid;
  gap: 24px;
  box-sizing: border-box;
  width: 100%;
  margin: 0 auto;
  padding: 0 24px;
  max-width: 1280px;
}

@media (min-width: 640px) {
  .demo {
    padding: 0 48px;
  }
}

@media (min-width: 960px) {
  .demo {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 32px;
    padding: 0 64px;
  }

  /* Side by side, the terminal takes the grid's height. */
  .terminal {
    min-height: 0;
  }
}

.terminal {
  display: flex;
  flex-direction: column;
  min-height: 240px;
  border: 2px solid var(--parcela-frame);
  border-radius: 22px;
  background: var(--vp-c-bg);
  box-shadow: var(--parcela-card);
  overflow: hidden;
}

.tabs {
  display: flex;
  align-items: stretch;
  min-height: 42px;
  padding: 0 8px;
  border-bottom: 1px solid var(--vp-c-divider);
  background: var(--parcela-slot);
}

.tab {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  padding: 0 14px;
  border: 0;
  border-right: 1px solid var(--vp-c-divider);
  background: none;
  color: var(--vp-c-text-3);
  font: inherit;
  font-size: 13px;
  font-weight: 700;
  line-height: 1;
  cursor: pointer;
  animation: appear 0.25s ease-out both;
  transition: background-color 0.2s ease, color 0.2s ease;
}

.tab:hover {
  color: var(--vp-c-text-1);
}

.tab.on {
  margin-bottom: -1px;
  background: var(--vp-c-bg);
  box-shadow: inset 0 3px 0 var(--color);
  color: var(--vp-c-text-1);
}

.player {
  display: grid;
  place-items: center;
  flex: none;
  width: 18px;
  height: 18px;
  border-radius: 6px;
  background: var(--color);
  color: #fff;
  font-size: 11px;
  font-weight: 800;
}

.tab svg {
  display: block;
  flex: none;
  width: 15px;
  height: 15px;
}

.tab-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.busy {
  flex: none;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--color);
}

.screen {
  position: relative;
  flex: 1;
}

/* New lines push old ones off the top, like a real terminal. */
pre {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  margin: 0;
  padding: 18px 22px;
  overflow: hidden;
  color: var(--vp-c-text-1);
  font-family: var(--vp-font-family-mono);
  font-size: 12px;
  line-height: 1.7;
  -webkit-mask-image: linear-gradient(transparent, #000 48px);
  mask-image: linear-gradient(transparent, #000 48px);
}

code {
  display: flex;
  flex-direction: column;
  white-space: pre;
}

.line {
  min-height: 1.7em;
}

.dim {
  color: var(--vp-c-text-3);
}

.ask {
  color: var(--vp-c-brand-1);
  font-weight: 600;
}

.say {
  color: var(--color);
}

.ok {
  color: #1f8a45;
  font-weight: 600;
}

.url {
  color: var(--vp-c-brand-1);
  font-weight: 600;
}

:root.dark .ok {
  color: #5fd68a;
}

.cursor {
  display: inline-block;
  width: 0.6em;
  height: 1.15em;
  margin-left: 1px;
  border-radius: 2px;
  background: var(--parcela-blue);
  vertical-align: -0.2em;
  animation: blink 1.1s steps(1) infinite;
}

.agents {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.grid {
  position: relative;
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
}

.slot {
  position: relative;
  aspect-ratio: 16 / 11;
  border: 2px solid var(--vp-c-divider);
  border-radius: 20px;
  background: var(--parcela-slot);
}

.tile {
  position: absolute;
  inset: -2px;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  padding: 11px 12px 10px;
  border: 2px solid var(--parcela-frame);
  border-radius: 20px;
  background: var(--vp-c-bg);
  box-shadow: var(--parcela-card);
  animation: appear 0.25s ease-out both;
  transition: border-color 0.3s ease, opacity 0.3s ease;
}

.tile.starting {
  border-color: var(--parcela-blue);
}

.tile.stopping {
  opacity: 0.5;
}

.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 4px;
  font-size: 13px;
}

.head strong {
  font-weight: 800;
}

.pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 1px 7px 1px 5px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 700;
  line-height: 1.6;
}

.pill.starting {
  background: #fff2d6;
  color: #9a6100;
}

.pill.ready {
  background: #e3f6e9;
  color: #1f8a45;
}

.pill.stopping {
  background: var(--parcela-slot);
  color: var(--vp-c-text-2);
}

.mark {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}

.host {
  overflow: hidden;
  color: var(--vp-c-brand-1);
  font-family: var(--vp-font-family-mono);
  font-size: 10px;
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.starting .host {
  color: var(--vp-c-text-3);
}


.pointer {
  position: absolute;
  z-index: 2;
  width: 0;
  height: 0;
  opacity: 0;
  transform: scale(0.6);
  transition: left 0.85s cubic-bezier(0.45, 0, 0.25, 1), top 0.85s cubic-bezier(0.45, 0, 0.25, 1), opacity 0.3s ease, transform 0.3s ease;
}

.pointer.shown {
  opacity: 1;
  transform: none;
}

.hand {
  position: absolute;
  left: -11px;
  top: -3px;
  width: 32px;
  overflow: visible;
  font-family: var(--vp-font-family-base);
  transform-origin: 11px 3px;
  animation: press 0.25s ease;
}

/* The console's loading ring: eight dots, the lit one going round. */
.loading {
  position: absolute;
  left: -34px;
  top: -2px;
  width: 22px;
  height: 22px;
  animation: appear 0.25s ease-out both;
}

.loading i {
  position: absolute;
  left: 9px;
  top: 9px;
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: var(--color);
  transform: rotate(calc(var(--dot) * 45deg)) translateY(-9px);
  animation: dot 0.8s steps(1) infinite;
  animation-delay: calc(var(--dot) * 0.1s - 0.8s);
}

@keyframes dot {
  0% { opacity: 1; }
  12.5% { opacity: 0.7; }
  25% { opacity: 0.45; }
  37.5% { opacity: 0.25; }
  50% { opacity: 0.15; }
}

.ring {
  position: absolute;
  left: -26px;
  top: -26px;
  width: 52px;
  height: 52px;
  border: 3px solid var(--color);
  border-radius: 50%;
  animation: ring 0.55s ease-out both;
}

@keyframes press {
  40% { scale: 0.88; }
}

@keyframes ring {
  from { opacity: 1; transform: scale(0.2); }
  to { opacity: 0; transform: scale(1); }
}

@keyframes appear {
  from { opacity: 0; transform: scale(0.96); }
}

@keyframes blink {
  50% { opacity: 0; }
}

@media (max-width: 480px) {
  .grid {
    gap: 8px;
  }

  .slot,
  .tile {
    border-radius: 14px;
  }

  .tile {
    padding: 7px 8px;
  }

  .head {
    font-size: 11px;
  }

  .pill {
    padding: 0;
    font-size: 0;
  }

  .host {
    font-size: 8px;
  }
}
</style>
