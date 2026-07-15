import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
const css = await readFile(new URL('./styles.css', import.meta.url), 'utf8');
const script = await readFile(new URL('./script.js', import.meta.url), 'utf8');

test('页面包含完整的核心叙事区块', () => {
  for (const id of ['top', 'product', 'engine', 'open-source', 'start']) {
    assert.match(html, new RegExp(`id="${id}"`));
  }
});

test('站内锚点都有对应目标', () => {
  const links = [...html.matchAll(/href="#([^"]+)"/g)].map((match) => match[1]);
  const ids = new Set([...html.matchAll(/id="([^"]+)"/g)].map((match) => match[1]));
  for (const target of links) assert.ok(ids.has(target), `缺少 #${target} 锚点`);
});

test('官网具有响应式与无障碍动态降级', () => {
  assert.match(css, /@media\(max-width:850px\)/);
  assert.match(css, /@media\(max-width:560px\)/);
  assert.match(css, /prefers-reduced-motion:reduce/);
  assert.match(html, /aria-label="主导航"/);
});

test('复制按钮和滚动显现交互已接入', () => {
  assert.match(script, /navigator\.clipboard\.writeText/);
  assert.match(script, /IntersectionObserver/);
  assert.match(html, /id="copyButton"/);
});
