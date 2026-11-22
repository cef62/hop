// Regenerates site/src/assets/screens/*.jpg. Needs a Playwright with WebKit (the app's engine) and `pnpm dev`
// running at the repo root. Example: NODE_PATH=/path/to/node_modules node capture.mjs  (or import from your install)
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
const { webkit } = createRequire(process.env.PLAYWRIGHT_FROM ?? import.meta.url)('playwright-core');
const here = (f) => fileURLToPath(new URL(f, import.meta.url));
const out = (f) => here(`../../src/assets/screens/${f}`);
const BASE = 'http://localhost:1420';
const mock = readFileSync(here('mock.js'), 'utf8');

const browser = await webkit.launch();
const desktop = (dark) => `linear-gradient(135deg, ${dark ? '#1b2a4a, #5b3a7a 55%, #b0507a' : '#8fb8f0, #c7a8f0 55%, #f4b6c8'})`;

for (const scheme of ['light', 'dark']) {
  const dark = scheme === 'dark';
  const ctx = await browser.newContext({ colorScheme: scheme, deviceScaleFactor: 2, viewport: { width: 640, height: 400 } });
  await ctx.addInitScript(mock);

  // palette: the real window is transparent over native vibrancy; fake it with a blurred card on a gradient desktop
  const p = await ctx.newPage();
  await p.goto(`${BASE}/index.html`);
  await p.addStyleTag({ content: `body::before{content:'';position:fixed;inset:0;z-index:-1;background:${desktop(dark)}}.card{backdrop-filter:blur(30px) saturate(1.8);box-shadow:0 0 0 .5px rgba(0,0,0,.25)}` });
  await p.waitForSelector('.row');
  await p.waitForTimeout(300);
  const shoot = async (f) => {
    const b = await p.locator('.card').boundingBox();
    await p.screenshot({ path: out(f), type: 'jpeg', quality: 90, clip: { x: 0, y: 0, width: 640, height: Math.ceil(b.y + b.height + 36) } });
  };
  await shoot(`palette-empty-${scheme}.jpg`);
  await p.fill('.search', 'sa');
  await p.waitForTimeout(300);
  await shoot(`palette-results-${scheme}.jpg`);
  await p.fill('.search', '12*7+3');
  await p.waitForTimeout(300);
  await shoot(`palette-calc-${scheme}.jpg`);
  // ⌘ alone only toggles the number hints; nothing runs until a second key
  await p.fill('.search', 'ma');
  await p.keyboard.down('Meta');
  await p.getByText('⌘2').waitFor(); // fail loudly if the held-⌘ hints stop rendering
  await p.waitForTimeout(300);
  await shoot(`palette-cmd-${scheme}.jpg`);
  await p.keyboard.up('Meta');

  // settings and what's new: capture the page at the real window size, then wrap it in a window frame
  for (const [name, page, w, h, title] of [['settings', 'settings.html', 520, 720, 'Settings'], ['whatsnew', 'whatsnew.html', 520, 560, "What's new"]]) {
    const c2 = await browser.newContext({ colorScheme: scheme, deviceScaleFactor: 2, viewport: { width: w, height: h } });
    await c2.addInitScript(mock);
    const s = await c2.newPage();
    await s.goto(`${BASE}/${page}`);
    // this WebKit build draws <input switch> as a plain box: restyle it like the macOS switch
    await s.addStyleTag({ content: `input[switch]{appearance:none;width:38px;height:22px;border-radius:11px;background:${dark ? '#5a5a5e' : '#d0d0d4'};position:relative;margin:0}input[switch]:checked{background:#34c759}input[switch]::before{content:"";position:absolute;top:2px;left:2px;width:18px;height:18px;border-radius:50%;background:#fff;box-shadow:0 1px 2px rgba(0,0,0,.3);transition:none}input[switch]:checked::before{left:18px}` });
    await s.waitForSelector('h1, h2');
    await s.waitForTimeout(400);
    const shot = (await s.screenshot()).toString('base64');
    await c2.close();
    const f = await ctx.newPage();
    await f.setViewportSize({ width: w + 80, height: h + 120 });
    const bar = dark ? '#323232' : '#e8e8e8';
    await f.setContent(`<body style="margin:0;background:${desktop(dark)};display:grid;place-items:center;height:100vh;font:13px -apple-system,sans-serif">
      <div style="border-radius:12px;overflow:hidden;box-shadow:0 20px 60px rgba(0,0,0,.4),0 0 0 .5px rgba(0,0,0,.35)">
        <div style="height:40px;background:${bar};display:flex;align-items:center;padding:0 14px;gap:8px;position:relative;color:${dark ? '#eee' : '#333'};font-weight:600">
          <i style="width:12px;height:12px;border-radius:50%;background:#ff5f57"></i><i style="width:12px;height:12px;border-radius:50%;background:#febc2e"></i><i style="width:12px;height:12px;border-radius:50%;background:#28c840"></i>
          <span style="position:absolute;left:0;right:0;text-align:center">${title}</span></div>
        <img src="data:image/png;base64,${shot}" width="${w}" height="${h}" style="display:block"></div></body>`);
    await f.evaluate(() => document.images[0].decode());
    await f.screenshot({ path: out(`${name}-${scheme}.jpg`), type: 'jpeg', quality: 90, clip: { x: 0, y: 0, width: w + 80, height: h + 120 } });
    await f.close();
  }
  await ctx.close();
}
await browser.close();
