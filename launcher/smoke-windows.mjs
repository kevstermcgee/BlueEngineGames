// CI-only CDP access to the real Windows WebView2. No debugging port ships in app settings.
import assert from "node:assert/strict";
import { writeFile, readFile, unlink, mkdir } from "node:fs/promises";
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
const nativeSnapshot = () =>
  evaluate(`window.__TAURI_INTERNALS__.invoke('snapshot')`);
const createdSaves = [];
let observingDownloads = false;
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
      `document.querySelector('#sidebar-toggle').getAttribute('aria-expanded')`,
    ),
    "true",
  );
  await evaluate(`document.querySelector('#sidebar-toggle').click();`);
  assert.equal(
    await evaluate(`document.documentElement.dataset.sidebarCollapsed`),
    "true",
  );
  assert.equal(
    await evaluate(
      `getComputedStyle(document.querySelector('main')).marginLeft`,
    ),
    "65px",
  );
  assert.equal(
    await evaluate(
      `document.querySelector('[data-view="library"]').getAttribute('aria-label')`,
    ),
    "My library",
  );
  assert.equal(
    await evaluate(
      `document.querySelector('#app-update').getBoundingClientRect().width>0`,
    ),
    true,
  );
  await call("Page.reload");
  await until(
    () => evaluate(`document.querySelectorAll('.card').length>=50`),
    "library after compact sidebar reload",
  );
  assert.equal(
    await evaluate(
      `document.querySelector('#sidebar-toggle').getAttribute('aria-expanded')`,
    ),
    "false",
  );
  assert.equal(
    await evaluate(
      `getComputedStyle(document.querySelector('main')).marginLeft`,
    ),
    "65px",
  );
  await mkdir("launcher-smoke", { recursive: true });
  const compact = await call("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  await writeFile(
    "launcher-smoke/windows-compact.png",
    Buffer.from(compact.data, "base64"),
  );
  await evaluate(`document.querySelector('#sidebar-toggle').click();`);
  console.log(
    "Compact sidebar navigation, layout and saved preference passed in Windows WebView2.",
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
  assert.equal(
    await evaluate(
      `document.querySelectorAll('#guide-feedback [data-feedback-form]').length`,
    ),
    1,
  );
  // Keep this synthetic review inside the isolated test profile. Verify the
  // browser handoff without posting fabricated player feedback to the repo.
  await evaluate(`(() => {
    const originalFetch = window.fetch;
    window.__feedbackSmoke = { originalFetch, urls: [] };
    // Tauri's invoke/ipc properties are immutable. Its custom protocol uses
    // the browser's fetch; intercept only this test's valid browser handoff.
    window.fetch = (input, options) => {
      const endpoint = new URL(typeof input === 'string' ? input : input.url, location.href);
      if (endpoint.hostname === 'ipc.localhost' && endpoint.pathname === '/open_feedback_url') {
        const payload = JSON.parse(options.body);
        if (payload.url.startsWith('https://github.com/kevstermcgee/BlueEngine/issues')) {
          window.__feedbackSmoke.urls.push(payload.url);
          return Promise.resolve(new Response('null', {
            headers: {'Content-Type': 'application/json', 'Tauri-Response': 'ok'}
          }));
        }
      }
      return originalFetch(input, options);
    };
    const form = document.querySelector('[data-feedback-form]');
    form.elements.rating.value = '8';
    form.elements.enjoyed.value = 'Smoke fixture: satisfying movement and clear characters.';
    form.elements.disliked.value = 'Smoke fixture: add clearer explanations for new players.';
    form.dispatchEvent(new Event('input', {bubbles: true}));
    form.elements.enjoyed.focus();
    form.elements.enjoyed.setSelectionRange(7, 14);
    window.__feedbackSmoke.form = form;
    document.querySelector('.favorite').click();
  })()`);
  assert.deepEqual(
    await evaluate(`({
      sameForm: document.querySelector('[data-feedback-form]') === window.__feedbackSmoke.form,
      focus: document.activeElement.name,
      selection: [document.activeElement.selectionStart, document.activeElement.selectionEnd]
    })`),
    { sameForm: true, focus: "enjoyed", selection: [7, 14] },
  );
  await evaluate(
    `document.querySelector('[data-feedback-form]').requestSubmit()`,
  );
  const reviewUrl = new URL(
    await until(
      () => evaluate(`window.__feedbackSmoke.urls[0]`),
      "feedback browser handoff",
    ),
  );
  assert.equal(reviewUrl.origin, "https://github.com");
  assert.equal(reviewUrl.pathname, "/kevstermcgee/BlueEngine/issues/new");
  assert.equal(reviewUrl.searchParams.get("game"), "embertrail");
  assert.equal(reviewUrl.searchParams.get("rating"), "8");
  assert.equal(
    reviewUrl.searchParams.get("format"),
    "blueengine-player-feedback-v1",
  );
  assert.equal(
    reviewUrl.searchParams.get("enjoyed"),
    "Smoke fixture: satisfying movement and clear characters.",
  );
  assert.equal(
    reviewUrl.searchParams.get("disliked"),
    "Smoke fixture: add clearer explanations for new players.",
  );
  assert(reviewUrl.searchParams.get("version"));
  assert.equal(
    await evaluate(`(async () => {
      try {
        await window.__TAURI_INTERNALS__.invoke('open_feedback_url', {url: 'file:///C:/Windows/System32/cmd.exe'});
        return false;
      } catch { return true; }
    })()`),
    true,
  );
  await evaluate(`document.querySelector('#close-guide').click()`);
  await until(
    () => evaluate(`!document.querySelector('#guide').open`),
    "closed feedback guide",
  );
  await evaluate(`document.querySelector('[data-guide="embertrail"]').click()`);
  assert.equal(
    await evaluate(
      `document.querySelector('[data-feedback-form]').elements.enjoyed.value`,
    ),
    "Smoke fixture: satisfying movement and clear characters.",
  );
  await evaluate(
    `document.querySelector('#guide-feedback').scrollIntoView({block: 'start'})`,
  );
  const feedbackScreenshot = await call("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  await writeFile(
    "launcher-smoke/windows-feedback.png",
    Buffer.from(feedbackScreenshot.data, "base64"),
  );
  await evaluate(`(() => {
    window.fetch = window.__feedbackSmoke.originalFetch;
    delete window.__feedbackSmoke;
    document.querySelector('#close-guide').click();
  })()`);
  await until(
    () => evaluate(`!document.querySelector('#guide').open`),
    "closed feedback after draft check",
  );
  await evaluate(
    `localStorage.removeItem('blueengine.feedback.v1.embertrail')`,
  );
  console.log(
    "Wiki feedback fields, whole review handoff, native URL restriction and saved drafts passed without posting a test review.",
  );
  await mkdir("launcher-smoke", { recursive: true });
  const screenshot = await call("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  await writeFile(
    "launcher-smoke/windows-library.png",
    Buffer.from(screenshot.data, "base64"),
  );
  const games = snapshot.catalog.games
    .filter(
      (g) =>
        g.package.kind === "zip" &&
        !snapshot.installed.some((i) => i.slug === g.slug && i.installed),
    )
    .sort((a, b) => a.package.size - b.package.size)
    .slice(0, 2);
  assert.equal(
    games.length,
    2,
    "Two uninstalled ZIP games are needed for smoke testing",
  );
  const saveContents = new Map();
  const saveName = `launcher-smoke-save-${process.pid}-${Date.now()}.txt`;
  for (const game of games) {
    await evaluate(
      `document.querySelector('[data-action="${game.slug}"]').click();`,
    );
    await until(
      () =>
        evaluate(
          `document.querySelector('[data-action="${game.slug}"]').textContent.includes('Play')`,
        ),
      `verified native install of ${game.slug}`,
      120000,
    );
    const installed = await nativeSnapshot();
    const receipt = installed.installed.find((i) => i.slug === game.slug);
    assert.equal(receipt.installed, true);
    assert.equal(
      receipt.release,
      installed.catalog.games.find((g) => g.slug === game.slug).package.release,
    );
    const save = join(receipt.path, saveName);
    const contents = `Retain this player-created save for ${game.slug}`;
    await writeFile(save, contents, { flag: "wx" });
    createdSaves.push(save);
    saveContents.set(game.slug, { path: save, contents });
    // Only age the release receipt; keep the verified payload and file manifest intact.
    await writeFile(
      join(receipt.path, ".installed-release"),
      `launcher-smoke-old-release-${game.slug}`,
    );
  }

  // Observe actual native phases, including transitions too quick for DOM polling.
  await evaluate(`(async () => {
    window.__bulkSmoke = { slugs: ${JSON.stringify(games.map((g) => g.slug))}, events: [] };
    const handler = window.__TAURI_INTERNALS__.transformCallback(({payload}) => {
      const state = window.__bulkSmoke;
      if (!state.slugs.includes(payload.slug)) return;
      const previous = state.events.at(-1);
      if (!previous || previous.slug !== payload.slug || previous.phase !== payload.phase)
        state.events.push({slug: payload.slug, phase: payload.phase, message: payload.message});
    });
    window.__bulkSmoke.eventId = await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {
      event: 'download', target: {kind: 'Any'}, handler
    });
  })()`);
  observingDownloads = true;
  await evaluate(
    `document.querySelector('#refresh').click();document.querySelector('[data-view="library"]').click();document.querySelector('#search').value='launcher-smoke-no-matching-game';document.querySelector('#search').dispatchEvent(new Event('input'));`,
  );
  await until(
    () =>
      evaluate(`(() => {
      const button = document.querySelector('#update-all');
      return button && !button.hidden && !button.disabled && /Update all\\s*\\(2\\)/i.test(button.textContent);
    })()`),
    "whole-library Update all (2) despite a zero-result search",
  );
  assert.equal(
    await evaluate(`document.querySelectorAll('.card').length`),
    0,
    "The active search must hide both installed games before Update all",
  );
  assert.equal(
    await evaluate(
      `document.querySelector('#update-all').getBoundingClientRect().width > 0`,
    ),
    true,
    "Update all must remain visible when the library search has no results",
  );
  await evaluate(`document.querySelector('#update-all').click();`);
  const events = await until(
    async () => {
      const events = await evaluate(`window.__bulkSmoke.events`);
      return events.some((e) => e.phase === "error") ||
        events.filter((e) => e.phase === "complete").length >= 2
        ? events
        : false;
    },
    "both queued native library updates to finish",
    240000,
  );
  let activeDownload = null;
  const completed = [];
  for (const event of events) {
    assert.notEqual(event.phase, "error", `${event.slug}: ${event.message}`);
    if (event.phase === "complete") {
      assert.equal(
        activeDownload,
        event.slug,
        "Only the active game can complete",
      );
      completed.push(event.slug);
      activeDownload = null;
    } else {
      assert(
        activeDownload === null || activeDownload === event.slug,
        "Update all must finish one game before starting the next",
      );
      activeDownload = event.slug;
    }
  }
  assert.equal(activeDownload, null);
  assert.deepEqual(completed.sort(), games.map((g) => g.slug).sort());
  const updated = await nativeSnapshot();
  for (const game of games) {
    const receipt = updated.installed.find((i) => i.slug === game.slug);
    const release = updated.catalog.games.find((g) => g.slug === game.slug)
      .package.release;
    assert.equal(receipt.installed, true);
    assert.equal(
      receipt.release,
      release,
      `${game.slug} must match the latest native catalog`,
    );
    assert.equal(
      await readFile(join(receipt.path, ".installed-release"), "utf8"),
      release,
    );
    const save = saveContents.get(game.slug);
    assert.equal(
      await readFile(save.path, "utf8"),
      save.contents,
      `${game.slug} save survived its update`,
    );
  }
  await evaluate(
    `document.querySelector('#search').value='';document.querySelector('#search').dispatchEvent(new Event('input'));`,
  );
  for (const game of games) {
    await until(
      () => evaluate(`!!document.querySelector('[data-guide="${game.slug}"]')`),
      `${game.slug} visible after clearing library search`,
    );
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
      `native uninstall of ${game.slug}`,
    );
    const save = saveContents.get(game.slug);
    assert.equal(
      await readFile(save.path, "utf8"),
      save.contents,
      `${game.slug} save survived uninstall`,
    );
    await evaluate(`document.querySelector('#close-guide').click();`);
  }
  console.log(
    `Windows IPC installs, sequential whole-library Update all under a zero-result search, latest receipts, and save-preserving confirmed uninstalls passed for ${games.map((g) => g.name).join(" and ")}.`,
  );
} finally {
  if (observingDownloads) {
    try {
      await evaluate(`(async () => {
        const eventId = window.__bulkSmoke.eventId;
        window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener('download', eventId);
        await window.__TAURI_INTERNALS__.invoke('plugin:event|unlisten', {event:'download', eventId});
        delete window.__bulkSmoke;
      })()`);
    } catch (error) {
      console.error("Could not detach the smoke download observer:", error);
      process.exitCode = 1;
    }
  }
  for (const save of createdSaves) {
    try {
      await unlink(save);
    } catch (error) {
      if (error.code !== "ENOENT") {
        console.error(`Could not remove test-created save ${save}:`, error);
        process.exitCode = 1;
      }
    }
  }
  socket.close();
}
