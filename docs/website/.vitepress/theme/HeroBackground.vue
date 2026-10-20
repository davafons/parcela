<script setup>
// A top-down field of land plots in box-drawing characters behind the home hero. It is the
// same on every render and mirrored around a center fence, so it stays symmetric at any width.
function field(plotsAcross, rows) {
  let seed = 7;
  const random = () => {
    seed = (seed * 1664525 + 1013904223) % 4294967296;
    return seed / 4294967296;
  };
  const w = 16;
  const h = 6;
  const crops = ['· · · · · · ·', "' ' ' ' ' ' '", '~ ~ ~ ~ ~ ~ ~', ': : : : : : :', '+   +   +   +', ''];
  const half = Math.ceil(plotsAcross / 2);
  const plots = Array.from({ length: Math.ceil(rows / h) }, () => Array.from({ length: half }, () => crops[Math.floor(random() * crops.length)]));
  const cols = plotsAcross * w + 1;
  const lines = [];
  for (let y = 0; y < rows; y += 1) {
    let line = '';
    for (let x = 0; x < cols; x += 1) {
      const edgeX = x % w === 0;
      const edgeY = y % h === 0;
      if (edgeX && edgeY) line += '┼';
      else if (edgeY) line += '─';
      else if (edgeX) line += '│';
      else {
        const column = Math.floor(x / w);
        const crop = plots[Math.floor(y / h)][Math.min(column, plotsAcross - 1 - column)];
        const row = (y % h) - 1;
        const char = row % 2 === 0 && row < h - 2 ? crop[(x % w) - 2] : undefined;
        line += char ?? ' ';
      }
    }
    lines.push(line);
  }
  return lines.join('\n');
}

const plots = field(22, 72);
</script>

<template>
  <div class="background" aria-hidden="true">
    <pre class="field">{{ plots }}</pre>
  </div>
</template>

<style scoped>
.background {
  position: absolute;
  z-index: -1;
  top: calc(var(--vp-nav-height) * -1);
  left: 0;
  right: 0;
  height: 1350px;
  overflow: hidden;
  pointer-events: none;
}

/* Clear behind the headline and the demo, fading out before the content below. */
.field {
  position: absolute;
  top: 0;
  left: 50%;
  margin: 0;
  color: var(--parcela-bg-mark);
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
  line-height: 18px;
  opacity: 0.8;
  transform: translateX(-50%);
  -webkit-mask-image: radial-gradient(ellipse 880px 540px at 50% 38%, transparent 55%, #000 100%), linear-gradient(#000 45%, transparent 80%);
  -webkit-mask-composite: source-in;
  mask-image: radial-gradient(ellipse 880px 540px at 50% 38%, transparent 55%, #000 100%), linear-gradient(#000 45%, transparent 80%);
  mask-composite: intersect;
}
</style>
