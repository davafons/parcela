<template>
  <figure class="terminal" aria-label="Two worktrees of the shop app running side by side">
    <div class="bar"><span></span><span></span><span></span></div>
    <pre><code><span class="comment"># your main checkout</span>
<span class="prompt">~/shop $</span> parcela up --detach
<span class="ok">✓</span> shop is running at <span class="url">https://shop.localhost:1443</span>

<span class="comment"># a second worktree, running at the same time</span>
<span class="prompt">~/shop $</span> git worktree add ../signup &amp;&amp; cd ../signup
<span class="prompt">~/signup $</span> parcela up --detach
<span class="ok">✓</span> signup (shop) is running at <span class="url">https://signup-shop.localhost:1443</span>

<span class="prompt">~/signup $</span> parcela ls
PROJECT  NAME     STATE  URL
shop     signup   <span class="ok">ready</span>  https://signup-shop.localhost:1443
shop     primary  <span class="ok">ready</span>  https://shop.localhost:1443</code></pre>
  </figure>
</template>

<style scoped>
.terminal {
  margin: 0;
  width: 100%;
  max-width: 560px;
  border-radius: 10px;
  background: #16161a;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.25);
  overflow: hidden;
}

.bar {
  display: flex;
  gap: 6px;
  padding: 10px 12px;
  background: #24242a;
}

.bar span {
  width: 11px;
  height: 11px;
  border-radius: 50%;
  background: #4a4a52;
}

pre {
  margin: 0;
  padding: 16px 18px 18px;
  overflow-x: auto;
  color: #dcdcd4;
  font-family: var(--vp-font-family-mono);
  font-size: 12.5px;
  line-height: 1.6;
}

.prompt {
  color: #8b8b94;
}

.comment {
  color: #e8a36b;
}

.ok {
  color: #6fcf8e;
}

.url {
  color: #7fd3e6;
  font-weight: 600;
}
</style>
