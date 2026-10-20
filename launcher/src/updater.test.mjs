import test from "node:test";
import assert from "node:assert/strict";
import { createLauncherUpdater } from "./updater.mjs";

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function resource(overrides = {}) {
  const calls = { download: [], install: 0, close: 0 };
  return {
    version: "0.2.0",
    calls,
    async download(callback, options) {
      calls.download.push({ callback, options });
    },
    async install() {
      calls.install += 1;
    },
    async close() {
      calls.close += 1;
    },
    ...overrides,
  };
}

function harness(overrides = {}) {
  const changes = [];
  const notices = [];
  const checks = [];
  let restarts = 0;
  const updater = createLauncherUpdater({
    check: async (options) => {
      checks.push(options);
      return null;
    },
    relaunch: async () => {
      restarts += 1;
    },
    onChange: (state) => changes.push({ ...state }),
    notify: (message) => notices.push(message),
    ...overrides,
  });
  return {
    updater,
    changes,
    notices,
    checks,
    get restarts() {
      return restarts;
    },
  };
}

test("startup check is immediate, while focus/online-like checks share a ten-minute limit", async () => {
  let clock = 0;
  const h = harness({ now: () => clock });
  await h.updater.checkForUpdates();
  assert.deepEqual(h.checks, [{ timeout: 20000 }]);
  assert.deepEqual(
    h.changes.map((state) => state.phase),
    ["checking", "idle"],
  );
  assert.deepEqual(h.notices, []);

  await h.updater.checkForUpdates();
  clock = 599999;
  await h.updater.checkForUpdates();
  assert.equal(h.checks.length, 1);
  clock = 600000;
  await h.updater.checkForUpdates();
  assert.equal(h.checks.length, 2);

  await h.updater.checkForUpdates({ force: true });
  assert.equal(h.checks.length, 3);
  await h.updater.checkForUpdates({ manual: true });
  assert.equal(h.checks.length, 4);
  assert.deepEqual(h.notices, ["Your launcher is up to date."]);
});

test("an offline startup remains quiet and a later transient failure preserves a known update", async () => {
  const available = resource();
  let attempt = 0;
  const h = harness({
    check: async () => {
      attempt += 1;
      if (attempt === 2) return available;
      throw new Error("network offline");
    },
  });
  await h.updater.checkForUpdates();
  assert.equal(h.updater.state.phase, "idle");
  assert.equal(h.updater.state.error, null);
  assert.deepEqual(h.notices, []);

  await h.updater.checkForUpdates({ force: true });
  await h.updater.checkForUpdates({ force: true });
  assert.equal(h.updater.state.phase, "available");
  assert.equal(h.updater.state.version, "0.2.0");
  assert.equal(available.calls.close, 0);
  assert.deepEqual(h.notices, []);

  await h.updater.checkForUpdates({ manual: true });
  assert.equal(h.updater.state.phase, "available");
  assert.match(
    h.notices[0],
    /Could not check launcher updates: .*network offline/,
  );
  await h.updater.install();
  assert.equal(available.calls.install, 1);
  assert.equal(h.restarts, 1);
});

test("successful checks close superseded native resources even when their versions match", async () => {
  const first = resource();
  const second = resource();
  const closeGate = deferred();
  first.close = async () => {
    first.calls.close += 1;
    await closeGate.promise;
  };
  const results = [first, second, second, null];
  const h = harness({ check: async () => results.shift() });
  await h.updater.checkForUpdates();
  const replacing = h.updater.checkForUpdates({ force: true });
  await Promise.resolve();
  assert.equal(first.calls.close, 1);
  assert.equal(h.updater.state.phase, "checking");
  closeGate.resolve();
  await replacing;
  assert.equal(h.updater.state.phase, "available");
  assert.equal(h.updater.state.version, second.version);

  await h.updater.checkForUpdates({ force: true });
  assert.equal(second.calls.close, 0, "an identical resource must remain open");
  await h.updater.checkForUpdates({ force: true });
  assert.equal(second.calls.close, 1);
  assert.equal(h.updater.state.phase, "idle");
  assert.equal(h.updater.state.version, null);
  await h.updater.install();
  assert.equal(
    second.calls.install,
    0,
    "an update removed by the feed cannot install",
  );
});

test("failure to close an old native resource does not discard its replacement", async () => {
  const first = resource({
    close: async () => {
      throw new Error("already closed");
    },
  });
  const next = resource({ version: "0.3.0" });
  const results = [first, next];
  const h = harness({ check: async () => results.shift() });
  await h.updater.checkForUpdates();
  await h.updater.checkForUpdates({ force: true });
  assert.equal(h.updater.state.phase, "available");
  assert.equal(h.updater.state.version, "0.3.0");
  await h.updater.install();
  assert.equal(first.calls.install, 0);
  assert.equal(next.calls.install, 1);
  assert.equal(h.restarts, 1);
});

test("checking excludes duplicate checks and installation of a previously known update", async () => {
  const available = resource();
  const checkGate = deferred();
  let checks = 0;
  const h = harness({
    check: async () => {
      checks += 1;
      return checks === 1 ? available : checkGate.promise;
    },
  });
  await h.updater.checkForUpdates();
  const checking = h.updater.checkForUpdates({ force: true });
  assert.equal(h.updater.state.phase, "checking");
  await h.updater.checkForUpdates({ manual: true, force: true });
  await h.updater.install();
  assert.equal(checks, 2);
  assert.equal(available.calls.download.length, 0);
  checkGate.resolve(available);
  await checking;
  await h.updater.install();
  assert.equal(available.calls.install, 1);
});

test("download and install exclude concurrent installs and update checks", async () => {
  const downloadGate = deferred();
  const installGate = deferred();
  const installationStarted = deferred();
  let downloads = 0;
  let installs = 0;
  let checks = 0;
  const available = resource({
    download: async () => {
      downloads += 1;
      await downloadGate.promise;
    },
    install: async () => {
      installs += 1;
      installationStarted.resolve();
      await installGate.promise;
    },
  });
  const h = harness({
    check: async () => {
      checks += 1;
      return available;
    },
  });
  await h.updater.checkForUpdates();
  const installing = h.updater.install();
  assert.equal(h.updater.state.phase, "downloading");
  assert.equal(h.updater.busy(), true);
  await h.updater.install();
  await h.updater.checkForUpdates({ manual: true, force: true });
  assert.equal(downloads, 1);
  assert.equal(checks, 1);

  downloadGate.resolve();
  await installationStarted.promise;
  assert.equal(h.updater.state.phase, "installing");
  await h.updater.install();
  await h.updater.checkForUpdates({ manual: true, force: true });
  assert.equal(downloads, 1);
  assert.equal(installs, 1);
  assert.equal(checks, 1);
  assert.equal(h.restarts, 0);
  installGate.resolve();
  await installing;
  assert.equal(h.restarts, 1);
});

test("a download signature failure cannot install or relaunch, and an explicit retry succeeds", async () => {
  let downloads = 0;
  const available = resource({
    download: async (onProgress, options) => {
      downloads += 1;
      assert.deepEqual(options, { timeout: 120000 });
      onProgress({ event: "Started", data: { contentLength: 100 } });
      onProgress({ event: "Progress", data: { chunkLength: 35 } });
      if (downloads === 1) throw new Error("invalid updater signature");
    },
  });
  const h = harness({ check: async () => available });
  await h.updater.checkForUpdates();
  await h.updater.install();
  assert.equal(available.calls.install, 0);
  assert.equal(h.restarts, 0);
  assert.equal(h.updater.busy(), false);
  assert.equal(h.updater.state.phase, "available");
  assert.match(h.updater.state.error, /invalid updater signature/);
  assert.equal(h.notices.length, 1);

  await h.updater.install();
  assert.equal(downloads, 2);
  assert.equal(available.calls.install, 1);
  assert.equal(h.restarts, 1);
  assert.equal(h.updater.state.error, null);
  const beginnings = h.changes.filter(
    (state) => state.phase === "downloading" && state.total === null,
  );
  assert.equal(beginnings.length, 2);
  assert.ok(
    beginnings.every((state) => state.received === 0),
    "a retry resets downloaded-byte progress",
  );
});

test("activity blocking prevents download, then permits installation once the activity ends", async () => {
  let reason = "Finish the running game before updating the launcher.";
  const available = resource();
  const h = harness({
    check: async () => available,
    blockReason: () => reason,
  });
  await h.updater.checkForUpdates();
  await h.updater.install();
  assert.deepEqual(h.notices, [reason]);
  assert.equal(available.calls.download.length, 0);
  assert.equal(available.calls.install, 0);
  assert.equal(h.updater.state.phase, "available");
  assert.equal(h.updater.busy(), false);
  reason = null;
  await h.updater.install();
  assert.equal(available.calls.install, 1);
  assert.equal(h.restarts, 1);
});

test("activity beginning during download prevents the disruptive install and preserves retry", async () => {
  let reason = null;
  let downloads = 0;
  const available = resource({
    download: async () => {
      downloads += 1;
      if (downloads === 1) reason = "A game download is active.";
    },
  });
  const h = harness({
    check: async () => available,
    blockReason: () => reason,
  });
  await h.updater.checkForUpdates();
  await h.updater.install();
  assert.equal(downloads, 1);
  assert.equal(available.calls.install, 0);
  assert.equal(h.restarts, 0);
  assert.equal(h.updater.state.phase, "available");
  assert.match(h.updater.state.error, /A game download is active/);
  reason = null;
  await h.updater.install();
  assert.equal(downloads, 2);
  assert.equal(available.calls.install, 1);
  assert.equal(h.restarts, 1);
});

test("progress accumulates byte chunks and installation precedes a successful restart", async () => {
  const order = [];
  const available = resource({
    download: async (progress) => {
      order.push("download");
      progress({ event: "Started", data: { contentLength: 100 } });
      progress({ event: "Progress", data: { chunkLength: 25 } });
      progress({ event: "Progress", data: { chunkLength: 75 } });
      progress({ event: "Finished" });
      order.push("verified");
    },
    install: async () => {
      assert.equal(h.updater.state.phase, "installing");
      order.push("install");
    },
  });
  const h = harness({
    check: async () => available,
    relaunch: async () => {
      order.push("relaunch");
    },
  });
  await h.updater.checkForUpdates();
  await h.updater.install();
  assert.deepEqual(order, ["download", "verified", "install", "relaunch"]);
  const progress = h.changes.filter((state) => state.phase === "downloading");
  assert.deepEqual(
    progress.map(({ received, total }) => [received, total]),
    [
      [0, null],
      [0, 100],
      [25, 100],
      [100, 100],
    ],
  );
  assert.equal(h.updater.state.version, "0.2.0");
  assert.equal(h.updater.state.received, 100);
  assert.equal(h.updater.state.error, null);
});

test("unknown download length remains indeterminate while bytes still advance", async () => {
  const available = resource({
    download: async (progress) => {
      progress({ event: "Started", data: {} });
      progress({ event: "Progress", data: { chunkLength: 8 } });
    },
  });
  const h = harness({ check: async () => available });
  await h.updater.checkForUpdates();
  await h.updater.install();
  assert.equal(h.updater.state.total, null);
  assert.equal(h.updater.state.received, 8);
  assert.equal(h.restarts, 1);
});

test("a native install failure never relaunches and leaves an actionable retry", async () => {
  let installs = 0;
  const available = resource({
    install: async () => {
      installs += 1;
      if (installs === 1) throw new Error("installer could not start");
    },
  });
  const h = harness({ check: async () => available });
  await h.updater.checkForUpdates();
  await h.updater.install();
  assert.equal(h.restarts, 0);
  assert.equal(h.updater.state.phase, "available");
  assert.equal(h.updater.busy(), false);
  assert.match(h.updater.state.error, /installer could not start/);
  await h.updater.install();
  assert.equal(installs, 2);
  assert.equal(h.restarts, 1);
});
