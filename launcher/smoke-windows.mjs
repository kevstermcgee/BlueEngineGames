// CI-only CDP access to the real Windows WebView2. No debugging port ships in app settings.
import assert from "node:assert/strict";
import { writeFile, access, unlink, mkdir } from "node:fs/promises";
import { join } from "node:path";
const delay = (ms) => new Promise((r) => setTimeout(r, ms));
async function until(fn, label, timeout = 60000) {
  const end = Date.now() + timeout;
  let last;
  while (Date.now() < end) {
    try {
      const value = await fn();
      if (value) return value;
    } catch (e) {
      last = e;
    }
    await delay(250);
  }
  throw new Error(`Timed out: ${label}${last ? `: ${last}` : ""}`);
}
const pages = await until(async () => {
  const p = await (await fetch("http://127.0.0.1:9223/json/list")).json();
  return p.find((p) => p.type === "page" && p.url.includes("tauri.localhost"));
}, "Windows WebView2 page");
const socket = new WebSocket(pages.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, { once: true });
  socket.addEventListener("error", reject, { once: true });
});
let serial = 0;
const pending = new Map();
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (!message.id) return;
  const p = pending.get(message.id);
  if (!p) return;
  pending.delete(message.id);
  if (message.error) p.reject(new Error(JSON.stringify(message.error)));
  else p.resolve(message.result);
});
const call = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = ++serial;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
async function evaluate(expression) {
  const result = await call("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails)
    throw new Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
}
try {
  await call("Page.enable");
  await call("Runtime.enable");
  await until(
    () => evaluate(`document.querySelectorAll('.card').length>=50`),
    "rendered game library",
  );
  const snapshot = await evaluate(
    `window.__TAURI_INTERNALS__.invoke('snapshot')`,
  );
  assert(snapshot.catalog.games.length >= 50);
  console.log(
    `Real Windows app rendered ${snapshot.catalog.games.length} games through native IPC.`,
  );
  assert.equal(
    await evaluate(
      `document.querySelector('#search').value='Vanguard';document.querySelector('#search').dispatchEvent(new Event('input'));document.querySelector('.title-button').textContent`,
    ),
    "BlueDM",
  );
  await evaluate(
    `document.querySelector('#search').value='';document.querySelector('#search').dispatchEvent(new Event('input'));document.querySelector('[data-guide="embertrail"]').click();`,
  );
  assert.equal(
    await evaluate(
      `document.querySelectorAll('.character-grid article').length`,
    ),
    16,
  );
  await until(
    () =>
      evaluate(
        `document.querySelector('#guide img').complete&&document.querySelector('#guide img').naturalWidth>0`,
      ),
    "offline guide screenshot",
  );
  await evaluate(`document.querySelector('#close-guide').click();`);
  await mkdir("launcher-smoke", { recursive: true });
  const screenshot = await call("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  await writeFile(
    "launcher-smoke/windows-library.png",
    Buffer.from(screenshot.data, "base64"),
  );
  const game = snapshot.catalog.games
    .filter((g) => g.package.kind === "zip")
    .sort((a, b) => a.package.size - b.package.size)[0];
  await evaluate(
    `document.querySelector('[data-action="${game.slug}"]').click();`,
  );
  await until(
    () =>
      evaluate(
        `document.querySelector('[data-action="${game.slug}"]').textContent.includes('Play')`,
      ),
    "verified native game install",
    120000,
  );
  const installed = await evaluate(
    `window.__TAURI_INTERNALS__.invoke('snapshot')`,
  );
  const receipt = installed.installed.find((i) => i.slug === game.slug);
  assert.equal(receipt.release, game.package.release);
  const save = join(receipt.path, "launcher-smoke-save.txt");
  await writeFile(save, "Retain this player-created save");
  await evaluate(
    `document.querySelector('[data-guide="${game.slug}"]').click();document.querySelector('[data-uninstall="${game.slug}"]').click();`,
  );
  assert.equal(
    await evaluate(`document.querySelector('#confirmation').open`),
    true,
  );
  await evaluate(`document.querySelector('#confirm-uninstall').click();`);
  await until(
    () =>
      evaluate(
        `document.querySelector('#guide [data-action="${game.slug}"]').textContent.includes('Install')`,
      ),
    "native game uninstall",
  );
  await access(save);
  await unlink(save);
  console.log(
    `Windows IPC install, receipt, guide, uninstall confirmation and save preservation passed for ${game.name}.`,
  );
} finally {
  socket.close();
}
