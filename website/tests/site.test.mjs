// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// The price grid on the home page: every catalogue service gets a name, art and the same "from" price the app shows.
// Run:  node --test website/tests/site.test.mjs   (no dependencies). Caddy answers /tests/* with 404, so nothing here is served.
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';

const site = new URL('../', import.meta.url);
const repo = new URL('../../', import.meta.url);
const read = (base, path) => readFileSync(new URL(path, base), 'utf8');

// The launch catalogue as GET /v1/services sends it (backend/crates/domain/src/catalog.rs default_catalog).
const unit = (price) => ({ label_key: 'x', minutes: 60, price });
const counts = (prices) => ({ type: 'counts', units: prices.map(unit), max_each: null, multi_discount_bps: 1000 });
const CATALOG = [
  { code: 'studio', pricing: { type: 'fixed', price: 45000, minutes: 120 } },
  { code: 'kitchen', pricing: { type: 'fixed', price: 45000, minutes: 120 } },
  { code: 'bathroom', pricing: { type: 'unit', units: [39000, 52000, 69000, 84000].map(unit) } },
  { code: 'mattress', pricing: counts([59000]) },
  { code: 'aircon', pricing: counts([79000, 119000, 179000, 139000]) },
  { code: 'sofa', pricing: counts([59000, 79000, 99000, 119000]) },
  { code: 'move_out', pricing: { type: 'quote' } },
  { code: 'move_in', pricing: { type: 'quote' } },
  { code: 'whole_home', pricing: { type: 'quote' } },
  { code: 'window', pricing: counts([9000, 15000]) },
  { code: 'disinfection', pricing: counts([80000, 90000, 100000, 300000]) },
];

// Just enough DOM for site.js to load and draw the grid.
function fakeElement() {
  const parts = {};
  return {
    children: [], style: { setProperty() {} }, textContent: '', innerHTML: '', className: '',
    append(...c) { this.children.push(...c); },
    addEventListener() {},
    querySelector(sel) { parts[sel] ??= fakeElement(); return parts[sel]; },
  };
}

function loadSite(lang) {
  const grid = fakeElement();
  const document = {
    documentElement: { dataset: {}, lang: 'ko' },
    querySelector: () => null, querySelectorAll: () => [], addEventListener() {},
    getElementById: (id) => (id === 'services-grid' ? grid : null),
    createElement: () => fakeElement(),
  };
  const ctx = vm.createContext({
    document, location: { hostname: 'shop.example' }, navigator: { language: lang },
    localStorage: { getItem: () => null, setItem() {} }, matchMedia: () => ({ matches: true }),
    fetch: () => Promise.reject(new Error('offline')), Intl, URL, URLSearchParams,
  });
  vm.runInContext(read(site, 'assets/site.js'), ctx);
  ctx.names = JSON.parse(read(site, 'assets/services.json'));
  return { grid, ctx, run: (code) => vm.runInContext(code, ctx) };
}

test('fromPrice matches the app: fixed price, cheapest unit or counts option, none for a quote', () => {
  const { ctx, run } = loadSite('ko');
  ctx.catalog = CATALOG;
  const prices = Object.fromEntries(run('catalog.map((s) => [s.code, fromPrice(s)])'));
  assert.deepEqual(prices, {
    studio: 45000, kitchen: 45000, bathroom: 39000, mattress: 59000, aircon: 79000, sofa: 59000,
    move_out: null, move_in: null, whole_home: null, window: 9000, disinfection: 80000,
  });
  assert.equal(run("fromPrice({ code: 'x', pricing: { type: 'counts', units: [] } })"), null);
  assert.equal(run("fromPrice({ code: 'x' })"), null);
});

test('the default catalogue is exactly the list the test prices', () => {
  const codes = [...read(repo, 'backend/crates/domain/src/catalog.rs').split('pub fn default_catalog')[1]
    .split('#[cfg(test)]')[0].matchAll(/service\(\s*"([a-z_]+)"/g)].map((m) => m[1]);
  assert.deepEqual(codes, CATALOG.map((s) => s.code));
});

for (const lang of ['ko', 'en']) {
  test(`every service card has a ${lang} name, its own art and a price line`, () => {
    const { grid, ctx, run } = loadSite(lang);
    ctx.catalog = CATALOG;
    run('services = catalog; renderServices()');
    assert.equal(grid.children.length, CATALOG.length);
    grid.children.forEach((card, i) => {
      const { code } = CATALOG[i];
      const name = card.querySelector('h3').textContent;
      assert.ok(name && name !== code, `${code}: shows "${name}"`);
      assert.ok(card.querySelector('.price').textContent, `${code}: no price line`);
      const src = run(`art(${JSON.stringify(code)})`);
      if (code !== 'whole_home') assert.notEqual(src, '/assets/icons3d/svc_whole_home.webp', `${code}: generic art`);
      assert.ok(existsSync(new URL(`.${src}`, site)), `${code}: ${src} missing`);
    });
  });
}

test('a service the site does not know yet gets a plain label and the generic art, never its code', () => {
  const { grid, ctx, run } = loadSite('ko');
  ctx.catalog = [{ code: 'balcony_x', pricing: counts([12000]) }];
  run('services = catalog; renderServices()');
  assert.equal(grid.children[0].querySelector('h3').textContent, '청소 서비스');
  assert.equal(run("art('balcony_x')"), '/assets/icons3d/svc_whole_home.webp');
});
