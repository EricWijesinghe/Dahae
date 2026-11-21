// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// Prints one manual book PDF (A4): cover, contents with page numbers, then every chapter.
// Rendered by Chromium through Playwright, so Korean uses the same shaping the browser uses.
//
//   node scripts/manual/book.js <ko|en> <YYYY-MM-DD> <fragment dir> <out.pdf>   (run through build.sh)
//
// Inputs:  <fragment dir>/<NN-name>.html, made by `md2html.py --fragment` from docs/manual/<lang>/<NN-name>.md
//          docs/manual/img/<lang>/*.png, app/packages/ui/assets/fonts, website/assets/favicon.svg
// Contents page numbers come from a first print: each listed heading carries a hidden marker, and
// pdftotext (poppler, ships with Git for Windows) reports which page each marker landed on.
// Playwright is loaded from SHOT_PLAYWRIGHT (env or scripts/local.env via env.sh; default: the `playwright` package).
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const { pathToFileURL } = require('url');
const { chromium } = require(process.env.SHOT_PLAYWRIGHT || 'playwright');

const ROOT = path.resolve(__dirname, '..', '..');
const MANUAL = path.join(ROOT, 'docs', 'manual');
const FONTS = path.join(ROOT, 'app', 'packages', 'ui', 'assets', 'fonts');
const LOGO = path.join(ROOT, 'website', 'assets', 'favicon.svg');
const EDITION = { ko: '제2판', en: '2nd edition' };
const COPYRIGHT = '© 2026 Eric Wijesinghe. All rights reserved.';
const PAGE_PLACEHOLDER = '000';
const MM = 96 / 25.4;
const CONTENT_PX = Math.floor((210 - 2 * 17) * MM); // keep in step with @page margin in book.css
const IMAGE_MAX_PX = Math.floor(125 * MM);

// Chapter number where each part starts (docs/manual/OUTLINE.md, chapter 00 table).
const PART_START = [0, 3, 8, 18, 24, 26, 28, 29, 30];
const TEXT = {
  ko: {
    title: '다해 사용 설명서',
    subtitle: '설치부터 매일 운영까지, 화면 그대로 따라 하는 안내서',
    edition: EDITION.ko,
    contents: '차례',
    missing: '그림 없음 · 위의 글 순서대로 따라 하세요',
    parts: ['1부 시작하기', '2부 서버 설치', '3부 관리자 콘솔', '4부 사업 운영', '5부 앱과 스토어',
      '6부 업데이트 · 백업 · 점검', '7부 브랜드', '8부 문제 해결', '9부 부록'],
    callouts: { '주의': 'warn', '팁': 'tip', '참고': 'note', '준비물': 'prep', '자주 생기는 문제': 'note', '확인 필요': 'warn' },
    ok: '이렇게 보이면 성공:',
    confirm: /확인 필요/g,
  },
  en: {
    title: 'Dahae User Manual',
    subtitle: 'From installing the server to running the business every day, step by step',
    edition: EDITION.en,
    contents: 'Contents',
    missing: 'No picture here · follow the written steps',
    parts: ['Part 1 Getting started', 'Part 2 Server install', 'Part 3 Admin console', 'Part 4 Running the business',
      'Part 5 Apps and stores', 'Part 6 Updates, backups, monitoring', 'Part 7 Brand', 'Part 8 Troubleshooting',
      'Part 9 Appendices'],
    callouts: { 'Warning': 'warn', 'Tip': 'tip', 'Note': 'note', 'Before you start': 'prep',
      'If something goes wrong': 'note', 'To be confirmed': 'warn' },
    ok: 'You should now see:',
    confirm: /to be confirmed/gi,
  },
};

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const stripTags = (s) => s.replace(/<[^>]+>/g, '').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');
const fileUrl = (p) => pathToFileURL(p).href;
// A CSS string literal; entities are not decoded inside <style>, so no HTML escaping here.
const cssString = (s) => `"${s.replace(/[\\"]/g, '\\$&').replace(/</g, '\\3c ')}"`;

// Ids are prefixed with the chapter file name so the concatenated book has no clashes, and links
// between chapter files (`18-catalog-prices.md#186-…`) become links inside the book.
function linkChapter(html, stem) {
  return html
    .replace(/ id="([^"]*)"/g, (_, id) => ` id="${stem}--${id}"`)
    .replace(/ href="#([^"]*)"/g, (_, id) => ` href="#${stem}--${id}"`)
    .replace(/ href="(\d\d-[a-z0-9-]+)\.md(?:#([^"]*))?"/g, (_, file, id) => ` href="#${id ? `${file}--${id}` : file}"`);
}

// The screenshots are taken at 2x, so a PNG shows at half its pixel width at most, no wider than the
// column and no taller than IMAGE_MAX_PX (tall phone and settings screens).
function retinaWidth(file) {
  const head = Buffer.alloc(24);
  const fd = fs.openSync(file, 'r');
  fs.readSync(fd, head, 0, 24, 0);
  fs.closeSync(fd);
  if (head.toString('latin1', 1, 4) !== 'PNG') return '';
  const w = head.readUInt32BE(16);
  const h = head.readUInt32BE(20);
  return ` style="width:${Math.floor(Math.min(w / 2, CONTENT_PX, (IMAGE_MAX_PX * w) / h))}px"`;
}

// Images that exist become file URLs; missing ones become a labelled box so the gap is visible.
function placeImages(html, lang, t) {
  const dir = path.join(MANUAL, lang);
  return html.replace(/<img src="([^"]*)" alt="([^"]*)">/g, (whole, src, alt) => {
    const file = path.resolve(dir, src);
    if (fs.existsSync(file)) return `<img src="${fileUrl(file)}" alt="${alt}"${retinaWidth(file)}>`;
    return `<div class="missing"><b>${esc(t.missing)}</b><span>${esc(path.basename(src))}</span></div>`;
  });
}

function decorate(html, t) {
  const keys = Object.keys(t.callouts).map((k) => k.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|');
  return html
    .replace(new RegExp(`<blockquote><p><strong>(${keys})</strong>`, 'g'),
      (_, k) => `<blockquote class="${t.callouts[k]}"><p><strong>${k}</strong>`)
    .replace(new RegExp(`<p><strong>${t.ok}</strong>`, 'g'), `<p class="ok"><strong>${t.ok}</strong>`)
    .replace(/>([^<]+)</g, (_, text) => `>${text.replace(t.confirm, (m) => `<mark>${m}</mark>`)}<`);
}

// Reads the chapters and lists what the contents page shows: every chapter and each numbered section.
function loadChapters(fragDir, lang, t) {
  const toc = [];
  const chapters = fs.readdirSync(fragDir).filter((f) => /^\d\d-.*\.html$/.test(f)).sort().map((file) => {
    const stem = file.replace(/\.html$/, '');
    let html = decorate(placeImages(linkChapter(fs.readFileSync(path.join(fragDir, file), 'utf8'), stem), lang, t), t);
    html = html.replace(/<h([12]) id="([^"]*)">(.*?)<\/h\1>/g, (whole, level, id, inner) => {
      const text = stripTags(inner);
      if (level === '2' && !/^\d+\.\d+\s/.test(text)) return whole;
      const n = toc.push({ level: Number(level), id, text, num: Number(stem.slice(0, 2)) });
      return `<h${level} id="${id}"><span class="mk">@@T${n}@@</span>${inner}</h${level}>`;
    });
    const h1 = html.match(/<h1 id="[^"]*">(?:<span class="mk">[^<]*<\/span>)?(.*?)<\/h1>/);
    return { stem, html, title: h1 ? stripTags(h1[1]) : stem };
  });
  if (!chapters.length) throw new Error(`no chapter fragments in ${fragDir}`);
  return { chapters, toc };
}

function fontFaces() {
  const weights = { Regular: 400, Medium: 500, SemiBold: 600, Bold: 700, ExtraBold: 800 };
  const faces = Object.entries(weights).map(([name, w]) =>
    `@font-face{font-family:"Dahae Sans";font-weight:${w};src:url("${fileUrl(path.join(FONTS, `DahaeSans-${name}.otf`))}")}`);
  // The full set covers rare Hangul the subset leaves out.
  faces.push(`@font-face{font-family:"Dahae Sans Full";src:url("${fileUrl(path.join(FONTS, 'full', 'DahaeSansFull-Regular.otf'))}")}`);
  return faces.join('\n');
}

function css(chapters, t) {
  const running = chapters.map((c, i) =>
    `@page ch${i}{@top-left{content:${cssString(c.title)}}}`).join('\n');
  return fs.readFileSync(path.join(__dirname, 'book.css'), 'utf8')
    .replace('/*FONTS*/', fontFaces())
    .replace('"/*BOOK_TITLE*/"', cssString(t.title))
    .replace('/*RUNNING*/', running);
}

function cover(t, date) {
  return `<section class="cover">
  <img class="logo" src="${fileUrl(LOGO)}" alt="">
  <h1 class="book-title">${esc(t.title)}</h1>
  <p class="book-sub">${esc(t.subtitle)}</p>
  <p class="book-edition">${esc(t.edition)} · ${esc(date)}</p>
  <p class="book-copy">${esc(COPYRIGHT)}</p>
</section>`;
}

function contents(toc, t, pages) {
  const rows = [];
  toc.forEach((e, i) => {
    const part = e.level === 1 ? PART_START.indexOf(e.num) : -1;
    if (part >= 0) rows.push(`<li class="part">${esc(t.parts[part])}</li>`);
    const page = pages ? pages[i] : PAGE_PLACEHOLDER;
    rows.push(`<li class="l${e.level}"><a href="#${e.id}"><span class="t">${esc(e.text)}</span><span class="dots"></span><span class="p">${page}</span></a></li>`);
  });
  return `<section class="toc"><h1 class="toc-title">${esc(t.contents)}</h1><ol>${rows.join('')}</ol></section>`;
}

function page(lang, t, date, book, pages) {
  const body = book.chapters.map((c, i) => `<section class="chapter" style="page:ch${i}">${c.html}</section>`).join('\n');
  return `<!doctype html><html lang="${lang}"><head><meta charset="utf-8"><title>${esc(t.title)}</title>
<style>${css(book.chapters, t)}</style></head><body>
${cover(t, date)}
${contents(book.toc, t, pages)}
${body}
</body></html>`;
}

// Blocks wider than the A4 text column (210 mm - 2 x 17 mm margins), laid out at that width.
async function overflowing(tab) {
  await tab.emulateMedia({ media: 'print' });
  await tab.setViewportSize({ width: CONTENT_PX, height: 1200 });
  return tab.evaluate(() => {
    const w = document.documentElement.clientWidth + 1;
    const wide = (e) => e.getBoundingClientRect().right > w;
    return [...document.querySelectorAll('.chapter *')]
      .filter((e) => wide(e) && !wide(e.parentElement))
      .map((e) => `${e.closest('.chapter').querySelector('h1').textContent.slice(0, 30)}: <${e.tagName.toLowerCase()}> ${e.textContent.slice(0, 60)}`);
  });
}

async function print(browser, html, htmlPath, pdfPath) {
  fs.writeFileSync(htmlPath, html);
  const tab = await browser.newPage();
  await tab.goto(fileUrl(htmlPath), { waitUntil: 'load' });
  await tab.evaluate(() => document.fonts.ready);
  const wide = await overflowing(tab);
  await tab.pdf({ path: pdfPath, preferCSSPageSize: true, printBackground: true, outline: true, tagged: true });
  await tab.close();
  return wide;
}

// Page texts of a PDF, split on form feeds; index 0 is page 1.
const pdfPages = (pdf) => execFileSync('pdftotext', ['-enc', 'UTF-8', pdf, '-'], { maxBuffer: 1 << 28 })
  .toString('utf8').split('\f').slice(0, -1);

function markerPages(pdf, count) {
  const pages = new Array(count).fill(null);
  pdfPages(pdf).forEach((text, p) => {
    for (const m of text.matchAll(/@@T(\d+)@@/g)) if (pages[m[1] - 1] === null) pages[m[1] - 1] = p + 1;
  });
  const lost = pages.findIndex((p) => p === null);
  if (lost >= 0) throw new Error(`contents entry ${lost + 1} not found in the first print`);
  return pages;
}

async function main() {
  const [lang, date, fragDir, out] = process.argv.slice(2);
  const t = TEXT[lang];
  if (!t || !/^\d{4}-\d{2}-\d{2}$/.test(date || '') || !fragDir || !out) {
    throw new Error('usage: node scripts/manual/book.js <ko|en> <YYYY-MM-DD> <fragment dir> <out.pdf>');
  }
  const book = loadChapters(fragDir, lang, t);
  const work = path.dirname(path.resolve(out));
  fs.mkdirSync(work, { recursive: true });
  const firstHtml = path.join(fragDir, `book-${lang}-1.html`);
  const firstPdf = path.join(fragDir, `book-${lang}-1.pdf`);
  const browser = await chromium.launch();
  try {
    const wide = await print(browser, page(lang, t, date, book, null), firstHtml, firstPdf);
    wide.forEach((w) => console.warn(`wider than the page: ${w}`));
    const pages = markerPages(firstPdf, book.toc.length);
    const finalHtml = page(lang, t, date, book, pages).replace(/<span class="mk">@@T\d+@@<\/span>/g, '');
    await print(browser, finalHtml, path.join(fragDir, `book-${lang}.html`), out);
  } finally {
    await browser.close();
  }
  const before = pdfPages(firstPdf).length;
  const total = pdfPages(out).length;
  // The markers are out of the flow, so dropping them must not move anything.
  if (before !== total) throw new Error(`page count changed between prints (${before} -> ${total})`);
  console.log(`${out}: ${total} pages, ${(fs.statSync(out).size / 1048576).toFixed(1)} MB`);
}

main().catch((e) => { console.error(e.message || e); process.exit(1); });
