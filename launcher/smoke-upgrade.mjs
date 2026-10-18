// CI-only signed-updater feed and WebView action. The production bundle uses HTTPS.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { createReadStream } from "node:fs";
import { readFile, stat, mkdir, writeFile } from "node:fs/promises";

const version = process.env.BLUEENGINE_UPGRADE_VERSION;
assert(version, "Expected production version is required");

if (process.argv[2] === "serve") {
  const installer = process.env.BLUEENGINE_UPGRADE_INSTALLER;
  assert(installer, "Expected signed production installer is required");
  const signature = (await readFile(installer + ".sig", "utf8")).trim();
  assert(signature, "Production installer signature is required");
  const { size } = await stat(installer);
  const corrupted = await readFile(installer);
  corrupted[Math.floor(corrupted.length / 2)] ^= 1;
  let manifestRequests = 0;
  let installerRequests = 0;
  const feed = {
    version,
    notes: "CI signed upgrade verification",
    platforms: {
      "windows-x86_64": {
        url: "http://127.0.0.1:9224/installer.exe",
        signature,
      },
    },
  };
  const server = createServer((request, response) => {
    response.setHeader("Cache-Control", "no-store");
    if (request.url === "/launcher-update.json") {
      manifestRequests += 1;
      response.setHeader("Content-Type", "application/json");
      response.end(JSON.stringify(feed));
    } else if (request.url === "/installer.exe") {
      installerRequests += 1;
      response.setHeader("Content-Type", "application/octet-stream");
      response.setHeader("Content-Length", size);
      // First request preserves the signed size but changes one byte. The real
      // plugin must reject it; the user's retry receives the genuine artifact.
      if (installerRequests === 1) response.end(corrupted);
      else createReadStream(installer).pipe(response);
    } else if (request.url === "/health") {
      response.setHeader("Content-Type", "application/json");
      response.end(
        JSON.stringify({ version, manifestRequests, installerRequests }),
      );
    } else {
      response.writeHead(404);
      response.end();
    }
  });
  server.listen(9224, "127.0.0.1", () =>
    console.log("CI signed update feed ready"),
  );
} else {
  const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  async function until(fn, label) {
    const end = Date.now() + 60000;
    let last;
    while (Date.now() < end) {
      try {
        const result = await fn();
        if (result) return result;
      } catch (error) {
        last = error;
      }
      await delay(250);
    }
    throw new Error(`Timed out: ${label}${last ? ` (${last})` : ""}`);
  }
  const page = await until(async () => {
    const pages = await (await fetch("http://127.0.0.1:9223/json/list")).json();
    return pages.find(
      (candidate) =>
        candidate.type === "page" && candidate.url.includes("tauri.localhost"),
    );
  }, "CI Windows WebView");
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  let serial = 0;
  const pending = new Map();
  socket.addEventListener("message", ({ data }) => {
    const message = JSON.parse(data);
    const operation = pending.get(message.id);
    if (!operation) return;
    pending.delete(message.id);
    if (message.error)
      operation.reject(new Error(JSON.stringify(message.error)));
    else operation.resolve(message.result);
  });
  socket.addEventListener("close", () => {
    for (const operation of pending.values())
      operation.reject(new Error("WebView closed"));
    pending.clear();
  });
  function call(method, params) {
    return new Promise((resolve, reject) => {
      const id = ++serial;
      pending.set(id, { resolve, reject });
      socket.send(JSON.stringify({ id, method, params }));
    });
  }
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
    assert.equal(
      await evaluate("window.__TAURI_INTERNALS__.invoke('plugin:app|version')"),
      "0.1.0",
    );
    await until(
      () =>
        evaluate(
          `!document.querySelector('#launcher-update').hidden && document.querySelector('#launcher-update-title').textContent.includes(${JSON.stringify(version)}) && !document.querySelector('#install-launcher').disabled`,
        ),
      "automatic signed update offer",
    );
    const snapshot = await evaluate(
      "window.__TAURI_INTERNALS__.invoke('snapshot')",
    );
    const marker = snapshot.root + "\\launcher-upgrade-player-save.txt";
    await writeFile(
      marker,
      "Keep player files across a signed launcher upgrade\n",
    );
    await mkdir("launcher-smoke", { recursive: true });
    await writeFile(
      "launcher-smoke/upgrade-before.json",
      JSON.stringify({ previousVersion: "0.1.0", version, marker }, null, 2),
    );
    const image = await call("Page.captureScreenshot", {
      format: "png",
      captureBeyondViewport: false,
    });
    await writeFile(
      "launcher-smoke/windows-update-available.png",
      Buffer.from(image.data, "base64"),
    );
    // The first signed download is deliberately corrupt. It must leave the old
    // application running and offer a retry rather than invoking the installer.
    await evaluate("document.querySelector('#install-launcher').click(); true");
    await until(
      () =>
        evaluate(
          "document.querySelector('#launcher-update-detail').textContent.toLowerCase().includes('signature') && !document.querySelector('#install-launcher').disabled",
        ),
      "real plugin signature rejection",
    );
    assert.equal(
      await evaluate("window.__TAURI_INTERNALS__.invoke('plugin:app|version')"),
      "0.1.0",
    );
    const rejected = await call("Page.captureScreenshot", {
      format: "png",
      captureBeyondViewport: false,
    });
    await writeFile(
      "launcher-smoke/windows-update-signature-rejected.png",
      Buffer.from(rejected.data, "base64"),
    );
    // Retry through the real user action; PowerShell verifies exit/replacement.
    await evaluate("document.querySelector('#install-launcher').click(); true");
    console.log(
      `Requested signed upgrade to ${version} through the actual app UI.`,
    );
  } finally {
    socket.close();
  }
}
