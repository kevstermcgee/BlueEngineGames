// Keep update checks independent of the library and gate the disruptive install.
export function createLauncherUpdater({
  check,
  relaunch,
  blockReason = () => null,
  onChange = () => {},
  notify = () => {},
  now = Date.now,
}) {
  let update = null;
  let lastCheck = null;
  let state = {
    phase: "idle",
    version: null,
    received: 0,
    total: null,
    error: null,
  };
  const publish = (changes) => {
    state = { ...state, ...changes };
    onChange(state);
  };
  const busy = () => ["downloading", "installing"].includes(state.phase);

  async function checkForUpdates({ manual = false, force = false } = {}) {
    if (busy() || state.phase === "checking") return;
    // Focus/online events may arrive together; don't repeatedly hit the feed.
    if (!manual && !force && lastCheck !== null && now() - lastCheck < 600000)
      return;
    lastCheck = now();
    publish({ phase: "checking", error: null });
    try {
      const next = await check({ timeout: 20000 });
      const previous = update;
      update = next;
      // Each successful check holds a native resource, even for the same version.
      if (previous && previous !== next) await previous.close().catch(() => {});
      publish({
        phase: next ? "available" : "idle",
        version: next?.version || null,
      });
      if (manual && !next) notify("Your launcher is up to date.");
    } catch (error) {
      publish({ phase: update ? "available" : "idle" });
      // An offline startup must not interrupt browsing or discard a known update.
      if (manual) notify(`Could not check launcher updates: ${error}`);
    }
  }

  async function install() {
    if (!update || busy() || state.phase === "checking") return;
    const blocked = blockReason();
    if (blocked) {
      notify(blocked);
      return;
    }
    publish({ phase: "downloading", received: 0, total: null, error: null });
    try {
      // Download verifies the signature before install can exit the Windows app.
      await update.download(
        (event) => {
          if (event.event === "Started")
            publish({ total: event.data.contentLength || null });
          if (event.event === "Progress")
            publish({ received: state.received + event.data.chunkLength });
        },
        { timeout: 120000 },
      );
      const blockedAfterDownload = blockReason();
      if (blockedAfterDownload) throw new Error(blockedAfterDownload);
      publish({ phase: "installing" });
      // NSIS exits this process and relaunches the installed application.
      await update.install({ restartAfterInstall: true });
      // Fallback for platforms/installers that return without exiting.
      await relaunch();
    } catch (error) {
      publish({
        phase: "available",
        error: `Launcher update failed: ${error}`,
      });
      notify("The launcher could not update. You can retry when you’re ready.");
    }
  }

  return {
    checkForUpdates,
    install,
    busy,
    get state() {
      return state;
    },
  };
}
