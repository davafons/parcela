<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { agents } from './agents.js';

// Three agents, each in a feature worktree of the `shop` project. An agent opens its terminal
// tab and its hand comes up under an empty slot; the hand keeps working there, `parcela up`
// drops its block into the slot, and `parcela down` takes it away and closes the tab. Clicking
// a block or a hand shows its tab.
const envs = [
  { name: 'fix-cart-total', host: 'fix-cart-total-shop.localhost', agent: agents[0] },
  { name: 'confirm-email', host: 'confirm-email-shop.localhost', agent: agents[1] },
  { name: 'fuzzy-search', host: 'fuzzy-search-shop.localhost', agent: agents[2] },
];
const players = ['#34bfed', '#ff5a5f', '#3cc46a'];
const url = (i) => `https://${envs[i].host}`;

// Each tab draws its transcript the way its agent's CLI does: the welcome banner, the markers
// before prompts, replies and commands, what leads command output, and the input box.
const harnesses = [
  {
    kind: 'claude', title: '✻ Welcome to Claude Code!', sub: 'cwd: ~/shop/.worktrees/fix-cart-total',
    ask: '> ', say: [['mark', '⏺ ']], tool: (cmd) => [['ok', '⏺ '], ['b', 'Bash'], ['', `(${cmd})`]],
    out: '  ⎿  ', more: '     ', prompt: '> ', hint: '? for shortcuts',
  },
  {
    kind: 'codex', title: '>_ OpenAI Codex', sub: 'directory: ~/shop/.worktrees/confirm-email',
    ask: '› ', say: [['dim', '• ']], tool: (cmd) => [['ok', '• '], ['b', 'Ran '], ['', cmd]],
    out: '  └ ', more: '    ', prompt: '› ', hint: '⏎ send',
  },
  {
    kind: 'cursor', title: 'Cursor Agent', sub: '~/shop/.worktrees/fuzzy-search',
    ask: '→ ', say: [['', '']], tool: (cmd) => [['mark', '⬢ '], ['b', 'Shell '], ['', cmd]],
    out: '  │ ', more: '  │ ', prompt: '→ ', hint: 'Auto · Add a follow-up',
  },
];

// ask: the person's prompt, typed. say: the agent's step. run: a shell command the agent runs,
// with its output. up/down: the run starts or stops that tab's worktree.
const script = [
  [0, 'ask', 'fix rounding in the checkout total'],
  [0, 'say', 'Cart#total now rounds once, at the end'],
  [0, 'up'],
  [1, 'ask', 'add email confirmation to signup'],
  [1, 'say', 'Added SignupMailer and a /confirm route'],
  [1, 'up'],
  [2, 'ask', 'make product search typo-tolerant'],
  [2, 'say', 'Added a trigram index to products.name'],
  [2, 'up'],
  [0, 'say', 'Checked /cart: $19.99, not $20.00'],
  [0, 'say', 'Tests pass. Ready for review.'],
  [1, 'run', 'parcela logs web --tail 1', [[['dim', 'web | '], ['', 'POST /signup 302, mail queued']]]],
  [1, 'say', 'Signed up and confirmed a test account'],
  [2, 'run', 'parcela status', [[['', 'shop fuzzy-search  '], ['ok', 'ready']], [['url', url(2)]]]],
  [2, 'say', '"shoos" finds Shoes. Tests pass.'],
  [2, 'down'],
  [1, 'down'],
  [0, 'down'],
];

const pointers = reactive(players.map(() => ({ shown: false, working: false, click: 0 })));
// The agent's last step before `down`: after it, the agent is done and its ring stops.
const last = envs.map((_, i) => script.findLastIndex(([tab, kind]) => tab === i && (kind === 'say' || kind === 'run')));

const tabs = reactive(envs.map(() => ({ open: false, busy: false, lines: [] })));
const order = ref([]);
const view = ref(-1);
const focus = ref(-1);
const typed = ref('');
const state = reactive(envs.map(() => 'empty'));
const root = ref(null);
// The machine grows a slot per agent at work instead of showing a fixed grid.
const slots = computed(() => envs.map((_, i) => i).filter((i) => tabs[i].open || state[i] !== 'empty'));
let run = 0;
let held = 0;
let observer;

// Clicking a tab, a block or a hand holds that tab on screen for a while; then the view
// follows the agents again.
function pick(i) {
  if (!tabs[i].open) return;
  view.value = i;
  pointers[i].click += 1;
  held = Date.now() + 8000;
}

function show(i) {
  focus.value = i;
  if (Date.now() > held || !tabs[view.value]?.open) view.value = i;
}

function open(i) {
  tabs[i].open = true;
  tabs[i].lines.push([['banner', '']]);
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

const ask = (i, text) => [['ask', harnesses[i].ask], ['user', text]];
const say = (i, text) => [...harnesses[i].say, ['', text]];
const shell = (i, cmd) => harnesses[i].tool(cmd);
const indent = (i, line, k = 0) => [['dim', k ? harnesses[i].more : harnesses[i].out], ...line];
const upLines = (i) => [shell(i, 'parcela up --detach'), indent(i, [['ok', '✓'], ['', ` ${envs[i].name} (shop) is running at `], ['url', url(i)]])];
const downLines = (i) => [shell(i, 'parcela down'), indent(i, [['ok', '✓'], ['', ` Stopped ${envs[i].name} (shop)`]])];
// Wrapped lines continue under the text, past the line's marker.
const hang = (line) => ({ '--hang': `${line[0][1].length}ch` });

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
      tabs[tab].lines.push(shell(tab, text), ...out.map((line, k) => indent(tab, line, k)));
    } else {
      tabs[tab].lines.push(kind === 'ask' ? ask(tab, text) : say(tab, text));
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
          lines.push(ask(tab, text));
          typed.value = '';
          tabs[tab].busy = true;
          pointers[tab].working = true;
          await wait(900);
        } else if (kind === 'say') {
          pointers[tab].click += 1;
          lines.push(say(tab, text));
          await wait(1100);
        } else if (kind === 'run') {
          pointers[tab].click += 1;
          lines.push(shell(tab, text));
          await wait(600);
          lines.push(...out.map((line, k) => indent(tab, line, k)));
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
        <pre v-if="view >= 0" :class="`h-${harnesses[view].kind}`"><code><template v-for="(line, i) in tabs[view].lines" :key="`${view}-${i}`"><span v-if="line[0][0] === 'banner'" class="banner"><b>{{ harnesses[view].title }}</b><span class="dim">{{ harnesses[view].sub }}</span></span><span v-else class="line" :style="hang(line)"><span v-for="([kind, text], j) in line" :key="j" :class="kind">{{ text }}</span></span></template><span class="input"><span class="ask">{{ harnesses[view].prompt }}</span>{{ view === focus ? typed : '' }}<span class="cursor"></span></span><span class="hint">{{ harnesses[view].hint }}</span></code></pre>
      </div>
    </div>
    <div class="zone" aria-hidden="true">
      <div class="zone-head">
        <svg viewBox="0 0 24 24"><path d="M4 5h16v11H4Z M2 19h20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" /></svg>
        <strong>Your machine</strong>
      </div>
      <div class="grid">
        <p v-if="!slots.length" class="empty">Each <code>parcela up</code> adds one</p>
        <div v-for="i in slots" :key="envs[i].name" class="cell" :style="{ '--color': players[i] }" @click="pick(i)">
          <div v-if="state[i] === 'empty'" class="ghost"></div>
          <div v-else class="tile" :class="[state[i], { on: i === view }]">
            <div class="head">
              <span class="status" :class="state[i]"></span>
              <strong>{{ envs[i].name }}</strong>
            </div>
            <svg class="app" viewBox="0 0 120 44" preserveAspectRatio="xMidYMid meet">
              <g v-if="i === 0">
                <g v-for="k in 3" :key="k" :transform="`translate(${(k - 1) * 41} 0)`">
                  <rect width="36" height="26" rx="5" class="soft" />
                  <rect y="31" width="26" height="4" rx="2" class="line-fill" />
                  <rect y="38" width="16" height="4" rx="2" class="accent" />
                </g>
              </g>
              <g v-else-if="i === 1">
                <rect width="120" height="11" rx="5.5" class="field" />
                <rect y="15" width="120" height="11" rx="5.5" class="field" />
                <rect y="31" width="120" height="13" rx="6.5" class="accent" />
              </g>
              <g v-else>
                <rect width="120" height="12" rx="6" class="field" />
                <g v-for="k in 3" :key="k" :transform="`translate(0 ${4 + k * 10})`">
                  <rect width="8" height="8" rx="2.5" class="soft" />
                  <rect x="12" y="1" :width="[64, 80, 52][k - 1]" height="3" rx="1.5" class="line-fill" />
                  <rect x="12" y="5.5" :width="[30, 36, 24][k - 1]" height="2.5" rx="1.25" class="line-fill faint" />
                </g>
              </g>
            </svg>
          </div>
          <div class="pointer" :class="{ shown: pointers[i].shown }">
            <span v-if="pointers[i].click" :key="pointers[i].click" class="ring"></span>
            <span v-if="pointers[i].working" class="loading"><i v-for="dot in 8" :key="dot" :style="{ '--dot': dot - 1 }"></i></span>
            <svg :key="`hand-${pointers[i].click}`" viewBox="-20 -6 64 86" class="hand">
              <g transform="translate(5 7)" opacity="0.16" stroke="#000" stroke-width="10" stroke-linejoin="round">
                <rect x="-8" y="0" width="16" height="48" rx="8" /><rect x="-12" y="30" width="46" height="42" rx="16" />
              </g>
              <g :fill="players[i]" :stroke="players[i]" stroke-width="9" stroke-linejoin="round">
                <rect x="-8" y="0" width="16" height="48" rx="8" /><rect x="-12" y="30" width="46" height="42" rx="16" />
              </g>
              <g fill="#fff"><rect x="-8" y="0" width="16" height="48" rx="8" /><rect x="-12" y="30" width="46" height="42" rx="16" /></g>
              <text x="11" y="61" text-anchor="middle" font-size="25" font-weight="800" :fill="players[i]">{{ i + 1 }}</text>
            </svg>
          </div>
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
    grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr);
    gap: 32px;
    padding: 0 64px;
  }

  /* Side by side, the terminal takes the machine's height. */
  .terminal {
    height: auto;
    min-height: 320px;
  }

  .grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

.terminal {
  display: flex;
  flex-direction: column;
  height: 260px;
  border: 1px solid var(--parcela-frame);
  border-radius: 22px;
  background: var(--vp-c-bg);
  box-shadow: var(--parcela-card);
  overflow: hidden;
}

.tabs {
  display: flex;
  align-items: stretch;
  min-height: 42px;
  padding: 6px 12px 0;
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
  border-radius: 12px 12px 0 0;
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
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.line {
  min-height: 1.7em;
  padding-left: var(--hang);
  text-indent: calc(-1 * var(--hang));
}

/* The harnesses: Claude Code's orange banner and boxed prompt, Codex's quiet bullets and
   shaded input, Cursor Agent's rounded box. */
.banner {
  display: flex;
  flex-direction: column;
  align-self: flex-start;
  margin-bottom: 10px;
  padding: 6px 14px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  white-space: pre-wrap;
}

.input {
  margin-top: 10px;
  padding: 4px 10px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
}

.hint {
  padding: 2px 10px 0;
  color: var(--vp-c-text-3);
  font-size: 0.92em;
}

.user {
  color: var(--vp-c-text-2);
}

.b {
  font-weight: 700;
}

.h-claude .banner {
  border-color: #d97757;
}

.h-claude .banner b,
.h-claude .ask {
  color: #d97757;
}

.h-claude .mark {
  color: var(--vp-c-text-1);
}

.h-codex .input {
  border-color: transparent;
  background: var(--parcela-slot);
}

.h-codex .ask {
  color: var(--vp-c-text-1);
}

.h-cursor .banner,
.h-cursor .input {
  border-radius: 10px;
}

.h-cursor .ask,
.h-cursor .mark {
  color: var(--vp-c-text-2);
}

.dim {
  color: var(--vp-c-text-3);
}

.ask {
  font-weight: 600;
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

/* The machine the terminal runs on: a slot per agent at work, its block once `up` is ready. */
.zone {
  border: 1px dashed var(--parcela-frame);
  border-radius: 22px;
  background: radial-gradient(var(--parcela-bg-mark) 1px, transparent 1.2px) 0 0 / 16px 16px;
}

.zone-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 14px 18px 0;
  color: var(--vp-c-text-2);
  font-size: 13px;
}

.zone-head svg {
  flex: none;
  width: 18px;
  height: 18px;
}

.zone-head strong {
  color: var(--vp-c-text-1);
  font-weight: 800;
}

.grid {
  position: relative;
  flex: 1;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 240px));
  justify-content: center;
  align-content: start;
  gap: 12px 24px;
  padding: 16px 16px 44px;
  min-height: 226px;
  box-sizing: border-box;
}

.empty {
  grid-column: 1 / -1;
  align-self: center;
  margin: 40px 0 0;
  color: var(--vp-c-text-3);
  font-size: 14px;
  text-align: center;
}

.empty code {
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
}

.cell {
  position: relative;
  aspect-ratio: 16 / 11;
  cursor: pointer;
  animation: appear 0.25s ease-out both;
}

.ghost,
.tile {
  position: absolute;
  inset: 0;
  border-radius: 18px;
}

.ghost {
  border: 1px dashed var(--parcela-frame);
  background: var(--parcela-slot);
}

.tile {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 12px;
  border: 1px solid var(--parcela-frame);
  background: var(--vp-c-bg);
  box-shadow: var(--parcela-card);
  animation: drop 0.35s cubic-bezier(0.34, 1.56, 0.64, 1) both;
  transition: border-color 0.3s ease, box-shadow 0.3s ease, opacity 0.3s ease;
}

.cell:hover .tile,
.tile.on {
  border-color: var(--color);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--color) 28%, transparent), var(--parcela-card);
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
  gap: 7px;
  font-size: 13px;
}

.head strong {
  overflow: hidden;
  font-weight: 800;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Green when ready, amber and pulsing while it starts, gray once it stops. */
.status {
  flex: none;
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: #9aa1a8;
  transition: background-color 0.3s ease;
}

.status.starting {
  background: #f0a000;
  animation: pulse 0.9s ease-in-out infinite;
}

.status.ready {
  background: #2fb35a;
  box-shadow: 0 0 0 3px rgba(47, 179, 90, 0.2);
}

@keyframes pulse {
  50% { opacity: 0.35; }
}

/* A sketch of the app this worktree serves. */
.app {
  flex: 1;
  min-height: 0;
  width: 100%;
}

.app .soft {
  fill: color-mix(in srgb, var(--color) 18%, transparent);
}

.app .field {
  fill: var(--parcela-slot);
  stroke: var(--parcela-frame);
  stroke-width: 1;
}

.app .line-fill {
  fill: var(--parcela-frame);
}

.app .faint {
  opacity: 0.6;
}

.app .accent {
  fill: var(--color);
}

.starting .app,
.stopping .app {
  opacity: 0.35;
}

/* Each agent's hand waits under its slot, its fingertip on the block's lower edge. */
.pointer {
  position: absolute;
  z-index: 2;
  left: 72%;
  top: calc(100% - 6px);
  width: 0;
  height: 0;
  opacity: 0;
  transform: translateY(12px) scale(0.6);
  transition: opacity 0.3s ease, transform 0.3s ease;
}

.pointer.shown {
  opacity: 1;
  transform: none;
}

.hand {
  position: absolute;
  left: -11px;
  top: -3px;
  width: 30px;
  overflow: visible;
  font-family: var(--vp-font-family-base);
  transform-origin: 11px 3px;
  animation: press 0.25s ease;
}

/* The console's loading ring: eight dots, the lit one going round. */
.loading {
  position: absolute;
  left: -36px;
  top: 8px;
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
  left: -22px;
  top: -22px;
  width: 44px;
  height: 44px;
  border: 3px solid var(--color);
  border-radius: 50%;
  box-sizing: border-box;
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

/* The block drops into its slot from the hand. */
@keyframes drop {
  from { opacity: 0; transform: translateY(14px) scale(0.85); }
}

@keyframes blink {
  50% { opacity: 0; }
}

@media (max-width: 480px) {
  .tab {
    gap: 6px;
    padding: 0 10px;
  }

  .tab svg,
  .tab:not(.on) .tab-name {
    display: none;
  }

  pre {
    padding: 14px 16px;
    font-size: 11px;
  }

  .zone-head {
    padding: 12px 14px 0;
  }

  .grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
    padding: 12px 12px 38px;
    min-height: 136px;
  }

  .ghost,
  .tile {
    border-radius: 14px;
  }

  .tile {
    gap: 4px;
    padding: 7px 8px;
  }

  .head {
    font-size: 11px;
  }

  .head {
    gap: 5px;
    align-items: flex-start;
  }

  /* Narrow blocks wrap their name at its hyphens rather than cut it. */
  .head strong {
    font-size: 10px;
    line-height: 1.25;
    white-space: normal;
  }

  .status {
    width: 7px;
    height: 7px;
    margin-top: 2px;
  }

  .hand {
    left: -9px;
    top: -2px;
    width: 24px;
  }

  .loading {
    left: -30px;
    top: 4px;
  }
}
</style>
