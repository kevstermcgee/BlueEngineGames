import { createLauncherUpdater } from "./updater.mjs";

export function initLauncherUpdates({
  native,
  check,
  relaunch,
  blockReason,
  notify,
}) {
  const $ = (selector) => document.querySelector(selector);
  const banner = document.createElement("section");
  banner.id = "launcher-update";
  banner.hidden = true;
  banner.setAttribute("aria-label", "Launcher update");
  banner.innerHTML = `<div><strong id="launcher-update-title"></strong><p id="launcher-update-detail" role="status"></p><progress id="launcher-update-progress" max="100" hidden aria-label="Launcher update download"></progress></div><button id="install-launcher">Update & restart</button><button id="dismiss-launcher-update" aria-label="Remind me next time" title="Remind me next time">×</button>`;
  $("main").prepend(banner);
  let dismissed = null;
  let installing = false;
  const controller = createLauncherUpdater({
    check,
    relaunch,
    blockReason,
    notify,
    onChange: render,
  });
  function render(s = controller.state) {
    const busy = ["downloading", "installing"].includes(s.phase);
    const checking = s.phase === "checking";
    const label = busy
      ? "Updating launcher…"
      : checking
        ? "Checking updates…"
        : s.version
          ? `Launcher ${s.version} available`
          : "Check launcher updates";
    $("#app-update-label").textContent = label;
    $("#app-update").title = label;
    $("#app-update").setAttribute("aria-label", label);
    $("#app-update").disabled = busy || checking;
    $("#app-update").classList.toggle("update-available", !!s.version);
    banner.hidden =
      !s.version || (!busy && !s.error && dismissed === s.version);
    $("#launcher-update-title").textContent = busy
      ? `Updating launcher to ${s.version}`
      : `Launcher ${s.version} is available`;
    const percent = s.total
      ? Math.min(100, Math.round((s.received / s.total) * 100))
      : null;
    $("#launcher-update-detail").textContent =
      s.error ||
      (s.phase === "downloading"
        ? `Downloading update${percent === null ? "…" : ` · ${percent}%`}`
        : s.phase === "installing"
          ? "Installing… The launcher will restart."
          : "Get the latest improvements. Your library and saves stay in place.");
    const progress = $("#launcher-update-progress");
    progress.hidden = s.phase !== "downloading";
    if (percent === null) progress.removeAttribute("value");
    else progress.value = percent;
    $("#install-launcher").textContent = s.error
      ? "Retry update"
      : busy
        ? "Updating…"
        : "Update & restart";
    $("#install-launcher").disabled = busy || checking;
    $("#dismiss-launcher-update").hidden = busy;
    // Game operations are guarded in app.js too; make the temporary pause visible.
    if (busy !== installing) {
      installing = busy;
      document.body.classList.toggle("launcher-updating", busy);
    }
  }
  $("#dismiss-launcher-update").onclick = () => {
    dismissed = controller.state.version;
    render();
  };
  $("#install-launcher").onclick = () => controller.install();
  $("#app-update").onclick = () => {
    if (!native)
      return notify("Launcher updates are available in the Windows app.");
    if (controller.state.version) {
      dismissed = null;
      render();
      $("#install-launcher").focus();
    } else controller.checkForUpdates({ manual: true });
  };
  render();
  if (native) {
    // Neither startup rendering nor library refresh waits for the update service.
    controller.checkForUpdates();
    setInterval(() => controller.checkForUpdates(), 60 * 60 * 1000);
    window.addEventListener("focus", () => controller.checkForUpdates());
    window.addEventListener("online", () =>
      controller.checkForUpdates({ force: true }),
    );
  }
  return controller;
}
