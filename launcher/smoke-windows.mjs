// CI-only CDP access to the real Windows WebView2. No debugging port ships in app settings.
import assert from "node:assert/strict";
import { writeFile, readFile, unlink, mkdir } from "node:fs/promises";
import { join } from "node:path";
import { createServer } from "node:http";
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
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
  if (message.method === "Fetch.requestPaused") {
    void call("Fetch.fulfillRequest", {
      requestId: message.params.requestId,
      responseCode: 404,
      responseHeaders: [
        { name: "Content-Type", value: "application/json" },
        { name: "Access-Control-Allow-Origin", value: "*" },
      ],
      body: Buffer.from(
        JSON.stringify({ error: "Unregistered import fixture" }),
      ).toString("base64"),
    });
    return;
  }
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
async function openGuideFromTitle(slug) {
  await evaluate(
    `document.querySelector('.title-button[data-guide="${slug}"]').click()`,
  );
  await until(
    () =>
      evaluate(
        `document.querySelector('#guide').open && document.querySelector('#guide').scrollTop === 0`,
      ),
    `${slug} title opens its guide at the top`,
  );
  await evaluate(
    `new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
  assert.equal(await evaluate(`document.querySelector('#guide').scrollTop`), 0);
}
async function closeGuide() {
  await evaluate(`document.querySelector('#close-guide').click()`);
  await until(
    () => evaluate(`!document.querySelector('#guide').open`),
    "closed game guide",
  );
}
async function expectFeedbackInView(label) {
  await until(
    () =>
      evaluate(`(() => {
        const dialog = document.querySelector('#guide');
        const bounds = dialog.getBoundingClientRect();
        const feedback = document.querySelector('#guide-feedback').getBoundingClientRect();
        return dialog.open && dialog.scrollTop > 0 && feedback.top >= bounds.top - 1 && feedback.top < bounds.bottom - 40;
      })()`),
    label,
  );
}
// The feedback-smoke Cargo feature confines these requests to loopback.
const feedbackSmoke = { reviews: [], fail: true };
const feedbackServer = createServer(async (request, response) => {
  response.setHeader("Content-Type", "application/json");
  if (request.method !== "POST" || request.url !== "/feedback") {
    response.writeHead(404).end(JSON.stringify({ error: "Not found" }));
    return;
  }
  try {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const review = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    feedbackSmoke.reviews.push(review);
    if (feedbackSmoke.fail)
      response.writeHead(503).end(JSON.stringify({ error: "Offline fixture" }));
    else
      response.end(
        JSON.stringify({ accepted: true, submission_id: review.submission_id }),
      );
  } catch {
    response.writeHead(400).end(JSON.stringify({ error: "Invalid fixture" }));
  }
});
const createdSaves = [];
let observingDownloads = false;
try {
  await new Promise((resolve, reject) => {
    feedbackServer.once("error", reject);
    feedbackServer.listen(9235, "127.0.0.1", resolve);
  });
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
  // Real Windows File paths travel through CDP's native drag data into WebView2,
  // not a synthetic JavaScript drop object. The canonical Python writer creates
  // this inert BAT's initial metadata block. Its body must never be launched.
  const artifact = join(snapshot.root, "unrelated-import-name.bat");
  const documentPath = join(snapshot.root, "portable-import-fixture.json");
  const executed = join(snapshot.root, "portable-import-must-not-execute.txt");
  const metadata = {
    format: "blueengine.game",
    schema_version: 1,
    game: {
      slug: "metadata-native-drop",
      title: "Embedded Windows Drop",
      version: "9.8.7",
      description: "Details carried by the actual dropped game file.",
      controls: "Arrows move",
      authors: ["A cousin"],
      tags: ["test"],
    },
    source: {
      repository: "kevstermcgee/BlueEngine",
      commit: "a".repeat(40),
      game_path: "games/metadata-native-drop",
    },
    engine: null,
  };
  await writeFile(documentPath, JSON.stringify(metadata));
  await writeFile(artifact, `@echo Unexpected execution>"${executed}"\r\n`);
  createdSaves.push(artifact, documentPath);
  execFileSync("python", [
    "-c",
    "import importlib.util,json,pathlib,sys; spec=importlib.util.spec_from_file_location('metadata',sys.argv[1]); m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m); m.embed(pathlib.Path(sys.argv[2]),json.loads(pathlib.Path(sys.argv[3]).read_text(encoding='utf-8')))",
    resolve("distribution/game_metadata.py"),
    artifact,
    documentPath,
  ]);
  await call("Fetch.enable", {
    patterns: [
      {
        urlPattern:
          "https://blue-engine.duckdns.org/community/v1/games/metadata-native-drop",
        requestStage: "Request",
      },
    ],
  });
  try {
    for (const type of ["dragEnter", "dragOver", "drop"])
      await call("Input.dispatchDragEvent", {
        type,
        x: 100,
        y: 100,
        data: { items: [], files: [artifact], dragOperationsMask: 1 },
      });
    await until(
      () =>
        evaluate(
          `document.querySelector('#creator').open && document.querySelector('#creator-content [data-community-action="create"] [name="slug"]').value === 'metadata-native-drop'`,
        ),
      "native file drop opens creator and fills its embedded identity",
    );
    const imported = await evaluate(
      `(() => { const form=document.querySelector('#creator-content [data-community-action="create"]'); return Object.fromEntries(new FormData(form)); })()`,
    );
    assert.equal(imported.title, metadata.game.title);
    assert.equal(imported.version, metadata.game.version);
    assert.equal(imported.source_commit, metadata.source.commit);
    assert.equal(imported.source_repository, metadata.source.repository);
    assert.equal(imported.source_game, metadata.game.slug);
    assert.ok(imported.note);
    const preview = await evaluate(
      `document.querySelector('#creator-content [data-community-imported]').textContent`,
    );
    for (const expected of [
      metadata.game.description,
      metadata.game.controls,
      "A cousin",
    ])
      assert.ok(preview.includes(expected));
    assert.equal(preview.includes("unrelated-import-name"), false);
    await assert.rejects(readFile(executed), { code: "ENOENT" });
    const image = await call("Page.captureScreenshot", {
      format: "png",
      captureBeyondViewport: false,
    });
    await mkdir("launcher-smoke", { recursive: true });
    await writeFile(
      "launcher-smoke/windows-game-file-import.png",
      Buffer.from(image.data, "base64"),
    );
    // Clear the prior result so this proves the picker performs a fresh import.
    await evaluate(
      `document.querySelector('#creator-content [data-community-action="create"] [name="title"]').value = ''`,
    );
    // The accessible picker follows the same parser and refill flow.
    const dom = await call("DOM.getDocument");
    const input = await call("DOM.querySelector", {
      nodeId: dom.root.nodeId,
      selector: "#creator-content [data-community-file]",
    });
    await call("DOM.setFileInputFiles", {
      nodeId: input.nodeId,
      files: [artifact],
    });
    await until(
      () =>
        evaluate(
          `document.querySelector('#creator-content [data-community-action="create"] [name="title"]').value === 'Embedded Windows Drop' && document.querySelector('#creator-content [data-community-status]').textContent.includes('Game details loaded')`,
        ),
      "native file picker metadata import",
    );
    await assert.rejects(readFile(executed), { code: "ENOENT" });
    await writeFile(
      "launcher-smoke/game-file-import-proof.json",
      JSON.stringify(
        {
          native_file_drop: true,
          picker: true,
          fields: imported,
          metadata_preview: true,
          untrusted_body_not_executed: true,
        },
        null,
        2,
      ),
    );
    await evaluate(`document.querySelector('#close-creator').click()`);
  } finally {
    await call("Fetch.disable");
  }
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
  assert.match(
    await evaluate(
      `document.querySelector('[data-view="favorites"] .nav-icon').textContent.trim()`,
    ),
    /^[★☆]$/u,
    "Favorites navigation uses a star",
  );
  const initiallyFavorite = await evaluate(
    `document.querySelector('.card [data-favorite="embertrail"]').getAttribute('aria-pressed') === 'true'`,
  );
  assert.equal(
    await evaluate(
      `document.querySelector('.card [data-favorite="embertrail"]').textContent.trim()`,
    ),
    initiallyFavorite ? "★" : "☆",
  );
  await evaluate(
    `document.querySelector('.card [data-favorite="embertrail"]').click()`,
  );
  assert.equal(
    await evaluate(
      `document.querySelector('.card [data-favorite="embertrail"]').getAttribute('aria-pressed') === 'true'`,
    ),
    !initiallyFavorite,
  );
  await call("Page.reload");
  await until(
    () => evaluate(`document.querySelectorAll('.card').length>=50`),
    "library after saving a favorite",
  );
  assert.equal(
    await evaluate(
      `document.querySelector('.card [data-favorite="embertrail"]').getAttribute('aria-pressed') === 'true'`,
    ),
    !initiallyFavorite,
    "Card favorite persists across reload",
  );
  await openGuideFromTitle("embertrail");
  assert.equal(
    await evaluate(
      `document.querySelector('#guide-favorite').getAttribute('aria-pressed') === 'true'`,
    ),
    !initiallyFavorite,
    "Guide favorite agrees with its card",
  );
  await evaluate(
    `document.querySelector('#guide-favorite').focus();document.querySelector('#guide-favorite').click()`,
  );
  assert.deepEqual(
    await evaluate(`({
      guide: document.querySelector('#guide-favorite').getAttribute('aria-pressed') === 'true',
      card: document.querySelector('.card [data-favorite="embertrail"]').getAttribute('aria-pressed') === 'true',
      stored: JSON.parse(localStorage.getItem('favorites') || '[]').includes('embertrail'),
      focus: document.activeElement.id
    })`),
    {
      guide: initiallyFavorite,
      card: initiallyFavorite,
      stored: initiallyFavorite,
      focus: "guide-favorite",
    },
    "Guide toggle updates the card and saved favorite while preserving focus",
  );
  assert.equal(
    await evaluate(
      `document.querySelector('.card [data-favorite="embertrail"]').textContent.trim()`,
    ),
    initiallyFavorite ? "★" : "☆",
  );
  await evaluate(
    `document.querySelector('#guide').scrollTop = document.querySelector('#guide').scrollHeight`,
  );
  assert(
    await evaluate(`document.querySelector('#guide').scrollTop > 1000`),
    "Guide is deeply scrolled before switching games",
  );
  await closeGuide();
  await openGuideFromTitle("pocket-breaker");
  await evaluate(
    `document.querySelector('#guide .guide-nav a[href="#guide-feedback"]').click()`,
  );
  await expectFeedbackInView("Feedback navigation deliberately scrolls down");
  await closeGuide();
  await openGuideFromTitle("embertrail");
  const guideOverview = await call("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  await writeFile(
    "launcher-smoke/windows-guide-overview.png",
    Buffer.from(guideOverview.data, "base64"),
  );
  await closeGuide();
  await evaluate(
    `document.querySelector('.public-score[data-guide="embertrail"][data-open-reviews]').click()`,
  );
  await expectFeedbackInView("Public score deliberately opens player reviews");
  await closeGuide();
  console.log(
    "Title-open scroll reset, deliberate Feedback jumps, and synchronized persistent star favorites passed in Windows WebView2.",
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
  // A close event is queued: reopening in the same event turn must not let that
  // earlier event detach the active guide's feedback submission listener.
  await evaluate(
    `document.querySelector('#close-guide').click();document.querySelector('.title-button[data-guide="embertrail"]').click();`,
  );
  await evaluate(
    `new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))`,
  );
  assert.equal(await evaluate(`document.querySelector('#guide').open`), true);
  // Real key events and IME composition must survive library renders. Restoring
  // focus after detaching a form can pass identity/selection checks while still
  // dropping keystrokes or committing an unfinished composition in WebView2.
  await evaluate(`(() => {
    const form = document.querySelector('[data-feedback-form]');
    const roots = ['#guide-feedback', '#guide-collaborate'].map(s => document.querySelector(s));
    const protectedNodes = new Set();
    for (const root of roots)
      for (let node = root; node && node !== document; node = node.parentNode)
        protectedNodes.add(node);
    const probe = window.__editingProbe = {
      form, detached: 0, blurred: 0, compositionEnded: 0,
      values: Object.fromEntries(['name', 'version', 'enjoyed', 'disliked'].map(name => [name, form.elements[name].value]))
    };
    probe.observer = new MutationObserver(records => {
      for (const record of records)
        for (const removed of record.removedNodes)
          if (protectedNodes.has(removed)) probe.detached++;
    });
    probe.observer.observe(document.querySelector('#guide-content'), {childList: true, subtree: true});
  })()`);
  for (const name of ["name", "version", "enjoyed", "disliked"]) {
    await evaluate(`(() => {
      const field = window.__editingProbe.form.elements[${JSON.stringify(name)}];
      field.value = '';
      field.focus();
    })()`);
    // Even count restores the original favorite after exercising changed guide
    // content on every key. JS clicks do not deliberately move keyboard focus.
    for (const key of "key test") {
      await call("Input.dispatchKeyEvent", { type: "keyDown", key, text: key });
      await evaluate(
        `document.querySelector('.card [data-favorite="embertrail"]').click()`,
      );
      await call("Input.dispatchKeyEvent", { type: "keyUp", key });
    }
    assert.deepEqual(
      await evaluate(`(() => {
        const probe = window.__editingProbe;
        const field = probe.form.elements[${JSON.stringify(name)}];
        return {value: field.value, focused: document.activeElement === field, detached: probe.detached};
      })()`),
      { value: "key test", focused: true, detached: 0 },
      `${name} accepts every native key while the library refreshes`,
    );
  }
  await evaluate(`(() => {
    const probe = window.__editingProbe;
    const field = probe.form.elements.enjoyed;
    field.value = '';
    field.focus();
    field.addEventListener('blur', () => probe.blurred++, {once: true});
    field.addEventListener('compositionend', () => probe.compositionEnded++);
  })()`);
  await call("Input.imeSetComposition", {
    text: "日本",
    selectionStart: 2,
    selectionEnd: 2,
  });
  await evaluate(
    `document.querySelector('.card [data-favorite="embertrail"]').click()`,
  );
  assert.deepEqual(
    await evaluate(`({
      focused: document.activeElement === window.__editingProbe.form.elements.enjoyed,
      value: window.__editingProbe.form.elements.enjoyed.value,
      detached: window.__editingProbe.detached,
      blurred: window.__editingProbe.blurred,
      compositionEnded: window.__editingProbe.compositionEnded
    })`),
    {
      focused: true,
      value: "日本",
      detached: 0,
      blurred: 0,
      compositionEnded: 0,
    },
    "A guide refresh does not interrupt active native IME composition",
  );
  await call("Input.insertText", { text: "日本" });
  await evaluate(
    `document.querySelector('.card [data-favorite="embertrail"]').click()`,
  );
  assert.equal(
    await evaluate(`window.__editingProbe.form.elements.enjoyed.value`),
    "日本",
    "The IME commits its complete text after the refresh",
  );
  await evaluate(`(() => {
    const probe = window.__editingProbe;
    probe.observer.disconnect();
    for (const [name, value] of Object.entries(probe.values)) probe.form.elements[name].value = value;
    probe.form.dispatchEvent(new Event('input', {bubbles: true}));
    delete window.__editingProbe;
  })()`);
  console.log(
    "Review text fields stayed connected and accepted every native key and IME composition across repeated guide refreshes.",
  );
  // The CI build posts through real native HTTP to the loopback fixture server.
  await evaluate(`(() => {
    window.__feedbackSmoke = {};
    const form = document.querySelector('[data-feedback-form]');
    form.elements.rating.value = '8';
    form.elements.name.value = '';
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
  const review = await until(
    () => feedbackSmoke.reviews[0],
    "review submitted through native HTTP to the loopback fixture",
  );
  assert.equal(review.game, "embertrail");
  assert.equal(review.rating, 8);
  assert.equal(review.name, "");
  assert.equal(review.format, "blueengine-player-feedback-v1");
  assert.match(review.submission_id, /^[a-f0-9-]{36}$/);
  assert.equal(
    review.enjoyed,
    "Smoke fixture: satisfying movement and clear characters.",
  );
  assert.equal(
    review.disliked,
    "Smoke fixture: add clearer explanations for new players.",
  );
  assert(review.version);
  await until(
    () =>
      evaluate(
        `!document.querySelector('[data-feedback-form] button[type="submit"]').disabled`,
      ),
    "failed submission enables retry",
  );
  await closeGuide();
  await evaluate(`document.querySelector('[data-guide="embertrail"]').click()`);
  assert.equal(
    await evaluate(
      `document.querySelector('[data-feedback-form]').elements.enjoyed.value`,
    ),
    review.enjoyed,
  );
  assert.equal(
    feedbackSmoke.reviews.length,
    3,
    "Transient failures receive bounded automatic retries",
  );
  for (const attempt of feedbackSmoke.reviews)
    assert.deepEqual(
      attempt,
      review,
      "Automatic retries preserve the same review and submission ID",
    );
  feedbackSmoke.fail = false;
  await evaluate(
    `document.querySelector('[data-feedback-form]').requestSubmit()`,
  );
  const retried = await until(
    () => feedbackSmoke.reviews[3],
    "saved draft retry",
  );
  assert.equal(retried.submission_id, review.submission_id);
  await until(
    () =>
      evaluate(`!localStorage.getItem('blueengine.feedback.v1.embertrail')`),
    "published receipt clears draft",
  );
  // A later review can have a display name; leave it as a saved offline draft.
  await evaluate(`(() => {
    const form = document.querySelector('[data-feedback-form]');
    form.elements.name.value = 'A player';
    form.elements.enjoyed.value = 'A second unsent review';
    form.dispatchEvent(new Event('input', {bubbles:true}));
  })()`);
  await closeGuide();
  await evaluate(`document.querySelector('[data-guide="embertrail"]').click()`);
  assert.equal(
    await evaluate(
      `document.querySelector('[data-feedback-form]').elements.name.value`,
    ),
    "A player",
  );
  assert.equal(
    await evaluate(`(async () => {
      try {
        await window.__TAURI_INTERNALS__.invoke('open_game_versions', {slug: 'not-a-real-game'});
        return false;
      } catch { return true; }
    })()`),
    true,
    "Unknown games cannot open a version-history destination",
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
    "Anonymous native HTTP review submission, idempotent retry, named drafts and receipt handling passed against the loopback fixture.",
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
  // Keep the two-game bulk check above intact, then exercise the guide's native
  // update action independently using only an isolated local receipt fixture.
  const guideGame = updated.catalog.games.find((g) => g.slug === games[0].slug);
  const guideInstallation = updated.installed.find(
    (i) => i.slug === guideGame.slug,
  );
  const metadataPath = join(guideInstallation.path, ".installed-metadata.json");
  const previousMetadata = JSON.parse(await readFile(metadataPath, "utf8"));
  const previousVersion = guideGame.versions?.find(
    (v) => v.release !== guideGame.package.release,
  );
  const agedMetadata = {
    ...previousMetadata,
    release:
      previousVersion?.release || `launcher-smoke-previous-${guideGame.slug}`,
    version: previousVersion?.name || "Local previous-release fixture",
    released_at: previousVersion?.released_at || null,
  };
  await writeFile(
    join(guideInstallation.path, ".installed-release"),
    agedMetadata.release,
  );
  await writeFile(metadataPath, JSON.stringify(agedMetadata));
  await evaluate(
    `window.__bulkSmoke.events = [];document.querySelector('#refresh').click()`,
  );
  await until(
    () =>
      evaluate(`document.querySelector('#update-count').textContent === '1'`),
    "one older installed release is available for the compact guide update",
  );
  await openGuideFromTitle(guideGame.slug);
  assert.equal(
    await evaluate(`document.querySelector('#guide-play').textContent.trim()`),
    "▶ Play",
  );
  assert.equal(
    await evaluate(`document.querySelector('#guide-game-update').disabled`),
    false,
  );
  const installedSummary = await evaluate(
    `document.querySelector('.game-release-summary').textContent`,
  );
  assert(installedSummary.includes(agedMetadata.version));
  assert.match(installedSummary, /Installed version/);
  assert.match(installedSummary, /Installed on/);
  assert.match(
    await evaluate(
      `document.querySelector('#guide-game-update').getAttribute('aria-label')`,
    ),
    /released|release date unavailable/i,
  );
  const compactUpdate = await call("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  await writeFile(
    "launcher-smoke/windows-game-update.png",
    Buffer.from(compactUpdate.data, "base64"),
  );
  await evaluate(`document.querySelector('#guide-game-update').click()`);
  const guideEvents = await until(
    async () => {
      const events = await evaluate(`window.__bulkSmoke.events`);
      return events.some(
        (e) =>
          e.slug === guideGame.slug && ["complete", "error"].includes(e.phase),
      )
        ? events
        : false;
    },
    "compact guide update to finish through native IPC",
    240000,
  );
  assert(!guideEvents.some((e) => e.phase === "error"));
  const guideUpdated = (await nativeSnapshot()).installed.find(
    (i) => i.slug === guideGame.slug,
  );
  assert.equal(guideUpdated.release, guideGame.package.release);
  assert(guideUpdated.installed_at >= previousMetadata.installed_at);
  assert.equal(guideUpdated.version, guideGame.version);
  await until(
    () =>
      evaluate(
        `!document.querySelector('#guide-game-update') && document.querySelector('#guide-play')?.textContent.trim() === '▶ Play' && !document.querySelector('#guide-play').disabled`,
      ),
    "updated guide keeps Play and removes the compact update action",
  );
  const guideSave = saveContents.get(guideGame.slug);
  assert.equal(await readFile(guideSave.path, "utf8"), guideSave.contents);
  await closeGuide();
  console.log(
    "Compact guide update installed the latest native release, exposed its installation date, retained Play and preserved the player save.",
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
  if (feedbackServer.listening)
    await new Promise((resolve) => feedbackServer.close(resolve));
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
