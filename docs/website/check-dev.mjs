import assert from 'node:assert/strict';
import { appendFileSync, readFileSync, realpathSync, unlinkSync, writeFileSync } from 'node:fs';
import { get } from 'node:https';
import { join } from 'node:path';
import { setTimeout } from 'node:timers/promises';
import { WebSocket } from 'ws';

const [origin, peer, directory, caFile, output] = process.argv.slice(2);
assert.equal(process.argv.length, 7, 'Usage: node check-dev.mjs ORIGIN PEER DISPOSABLE_SOURCE CA REPORT');
const source = realpathSync(directory);
assert.equal(JSON.parse(readFileSync(join(source, '.docs-dev-fixture.json'))).source, source);
for (const value of [origin, peer]) {
  const url = new URL(value);
  assert.equal(url.protocol, 'https:');
  assert.ok(url.hostname.endsWith('.localhost'));
}
const ca = readFileSync(caFile);
const lookup = (_host, options, callback) => options.all
  ? callback(null, [{ address: '127.0.0.1', family: 4 }]) : callback(null, '127.0.0.1', 4);
const report = { complete: false, origin, peer, verified: [], messages: [] };
writeFileSync(output, JSON.stringify(report, null, 2) + '\n');
const messages = [];
const index = join(source, 'docs/site/index.md');
const skill = join(source, '.agents/skills/parcela/SKILL.md');
const css = join(source, 'docs/website/src/parcela.css');
const originals = new Map([index, skill, css].map((path) => [path, readFileSync(path)]));
const unlisted = join(source, 'docs/site/private-dev-fixture.md');
let unlistedCreated = false;
let socket;

function request(base, path) {
  return new Promise((resolve, reject) => {
    const request = get(new URL(path, base), { ca, lookup }, (response) => {
      const chunks = [];
      response.on('data', (data) => chunks.push(data));
      response.on('error', reject);
      response.on('end', () => resolve({ status: response.statusCode, body: Buffer.concat(chunks).toString() }));
    });
    request.on('error', reject);
    request.setTimeout(5000, () => request.destroy(new Error('HTTPS request deadline exceeded')));
  });
}

async function until(check) {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    if (await check()) return;
    await setTimeout(100);
  }
  throw new Error('Live documentation predicate did not become true');
}

try {
  const client = await request(origin, '/@vite/client');
  assert.equal(client.status, 200);
  const token = /const wsToken = ("[^"]+")/.exec(client.body);
  assert.ok(token, 'Native Vite WebSocket token missing');
  const url = new URL('/?token=' + encodeURIComponent(JSON.parse(token[1])), origin);
  url.protocol = 'wss:';
  socket = new WebSocket(url, 'vite-hmr', { ca, lookup, handshakeTimeout: 5000 });
  socket.on('message', (data) => messages.push(JSON.parse(data.toString())));
  await new Promise((resolve, reject) => { socket.once('open', resolve); socket.once('error', reject); });
  await until(() => messages.some((message) => message.type === 'connected'));
  report.verified.push('CA-verified named HTTPS and native Vite WSS connection through Caddy');
  for (const path of ['/index.md', '/operating-skill/index.md', '/@fs' + css + '?import']) {
    assert.equal((await request(origin, path)).status, 200, path);
  }
  const peerIndex = (await request(peer, '/index.md')).body;
  const peerSkill = (await request(peer, '/assets/parcela-skill.txt')).body;
  const marker = 'PARCELA_LIVE_CANONICAL_MARKER_73941';
  let start = messages.length;
  appendFileSync(index, '\n\n' + marker + '\n');
  await until(async () => (await request(origin, '/index.md')).body.includes(marker));
  await until(() => messages.slice(start).some((message) => message.type === 'custom'
    && message.event === 'vitepress:pageData' && message.data.path === '/index.md'));
  assert.equal((await request(peer, '/index.md')).body, peerIndex);
  report.verified.push('Canonical Markdown refreshes compiled content and native VitePress page data, without peer changes');
  start = messages.length;
  appendFileSync(skill, '\n\nPARCELA_LIVE_SKILL_MARKER_73941\n');
  await until(async () => (await request(origin, '/assets/parcela-skill.txt')).body === readFileSync(skill, 'utf8'));
  await until(async () => (await request(origin, '/operating-skill/index.md')).body.includes('PARCELA_LIVE_SKILL_MARKER_73941'));
  assert.equal((await request(peer, '/assets/parcela-skill.txt')).body, peerSkill);
  report.verified.push('Canonical hidden skill updates both its rendered page and exact download, without peer changes');
  start = messages.length;
  appendFileSync(css, '\n.parcela-live-css-proof { color: rgb(1, 2, 3); }\n');
  await until(async () => (await request(origin, '/@fs' + css + '?import')).body.includes('parcela-live-css-proof'));
  await until(() => messages.slice(start).some((message) => message.type === 'update'
    && message.updates.some((update) => update.path.includes('parcela.css'))));
  report.verified.push('Native stylesheet HMR travels through the same WSS connection');
  const validIndex = readFileSync(index);
  start = messages.length;
  appendFileSync(index, '\n\n{{ 7 * 7 }}\n');
  await until(() => messages.slice(start).some((message) => message.type === 'error'
    && message.err.message.includes('Public Markdown cannot load extra sources')));
  assert.ok((await request(origin, '/index.md')).body.includes(marker));
  assert.ok(!(await request(origin, '/index.md')).body.includes('{{ 7 * 7 }}'));
  start = messages.length;
  writeFileSync(index, validIndex);
  await until(() => messages.slice(start).some((message) => message.type === 'full-reload'));
  report.verified.push('Invalid templates leave last-good generated content intact and recover after correction');
  start = messages.length;
  writeFileSync(unlisted, '# UNDECLARED_DOCS_DEV_SENTINEL_73941\n', { flag: 'wx' });
  unlistedCreated = true;
  await until(() => messages.slice(start).some((message) => message.type === 'error'
    && message.err.message.includes('Undeclared public documentation source')));
  assert.ok(!(await request(origin, '/private-dev-fixture.md')).body.includes('UNDECLARED_DOCS_DEV_SENTINEL_73941'));
  start = messages.length;
  unlinkSync(unlisted);
  unlistedCreated = false;
  await until(() => messages.slice(start).some((message) => message.type === 'full-reload'));
  report.verified.push('Undeclared public input is refused during live watching and recovers after removal');
  assert.ok(messages.filter((message) => message.event === 'vitepress:pageData')
    .every((message) => ['/index.md', '/operating-skill/index.md'].includes(message.data.path)),
    'Canonical source paths escaped into native rendering/search events');
  report.verified.push('Only validated generated page paths enter native rendering/search events');
  report.complete = true;
} catch (error) {
  report.error = error.message;
} finally {
  try {
    if (unlistedCreated) unlinkSync(unlisted);
    for (const [path, bytes] of originals) writeFileSync(path, bytes);
    await until(async () => !(await request(origin, '/index.md')).body.includes('PARCELA_LIVE_CANONICAL_MARKER_73941'));
  } catch (error) {
    report.complete = false;
    report.restore_error = error.message;
  }
  socket?.terminate();
  report.messages = messages.map(({ type, event, data, updates, err }) => ({
    type, event, path: data?.path, updates: updates?.map(({ type, path }) => ({ type, path })), error: err?.message,
  }));
  writeFileSync(output, JSON.stringify(report, null, 2) + '\n');
}
assert.ok(report.complete, `Live docs check failed; inspect ${output}`);
console.log('PASS native documentation generation, HMR, source refusals and peer isolation');
