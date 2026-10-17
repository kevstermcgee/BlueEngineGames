import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { filterGames, status, size } from "./model.mjs";
const $ = (s) => document.querySelector(s),
  native = !!window.__TAURI_INTERNALS__;
let imageMap = {},
  pendingUpdate = null;
let games = [],
  installs = {},
  options = {
    view: "discover",
    query: "",
    origin: "",
    format: "",
    sort: "newest",
    favorites: readFavorites(),
  },
  active = null,
  queue = [],
  guideSlug = null,
  uninstallSlug = null,
  updaterBusy = false;
function readFavorites() {
  try {
    const value = JSON.parse(localStorage.getItem("favorites") || "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}
const esc = (s) =>
  String(s ?? "").replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ],
  );
const img = (s) => {
  try {
    const u = new URL(s);
    return u.origin === "https://kevstermcgee.github.io" &&
      u.pathname.startsWith("/BlueEngineGames/")
      ? esc(imageMap[u.href] || u.href)
      : "";
  } catch {
    return "";
  }
};
const getGame = (s) => games.find((g) => g.slug === s),
  state = (g) => status(g, installs[g.slug]);
function toast(message) {
  $("#toast").textContent = message;
  $("#toast").hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => ($("#toast").hidden = true), 8000);
}
const actionLabel = (g) =>
  active === g.slug
    ? "Installing…"
    : queue.includes(g.slug)
      ? "Queued"
      : {
          available: "↓ Install",
          installed: "▶ Play",
          update: "↓ Update",
          running: "Playing",
        }[state(g)];
function primary(g) {
  return `<button class="primary" data-action="${esc(g.slug)}" ${active === g.slug || queue.includes(g.slug) || state(g) === "running" ? "disabled" : ""}>${actionLabel(g)}</button>${state(g) === "update" ? `<button class="play-installed" data-play="${esc(g.slug)}">▶ Play installed</button>` : ""}`;
}
function badges(g) {
  return `<span class="badge">${g.origin === "ai" ? "AI-designed" : "Human-designed"}</span>${g.presentation ? `<span class="badge">${esc(g.presentation.toUpperCase())}</span>` : ""}${state(g) === "update" ? '<span class="badge update-badge">Update available</span>' : ""}`;
}
function card(g) {
  return `<article class="card"><button class="card-image" data-guide="${esc(g.slug)}" aria-label="Explore ${esc(g.name)}"><img src="${img(g.image)}" alt="${esc(g.name)} gameplay" loading="lazy">${state(g) === "installed" ? '<span class="installed-badge">✓ In your library</span>' : ""}</button><div class="card-body"><div class="card-top"><button class="title-button" data-guide="${esc(g.slug)}">${esc(g.name)}</button><button class="favorite ${options.favorites.includes(g.slug) ? "chosen" : ""}" data-favorite="${esc(g.slug)}" aria-label="Favorite ${esc(g.name)}" aria-pressed="${options.favorites.includes(g.slug)}">${options.favorites.includes(g.slug) ? "♥" : "♡"}</button></div><div class="badges">${badges(g)}</div><p class="description">${esc(g.description)}</p><div class="card-footer">${primary(g)}<span>${size(g.package.size)}</span><button class="guide-link" data-guide="${esc(g.slug)}">Game guide ↗</button></div></div></article>`;
}
function render() {
  const filtered = filterGames(games, installs, options);
  $("#grid").innerHTML = filtered.map(card).join("");
  $("#results").textContent =
    `${filtered.length} ${filtered.length === 1 ? "game" : "games"}`;
  $("#empty").hidden = !!filtered.length;
  $("#total-count").textContent = games.length;
  $("#library-count").textContent = games.filter(
    (g) => state(g) !== "available",
  ).length;
  $("#update-count").textContent = games.filter(
    (g) => state(g) === "update",
  ).length;
  const views = {
    discover: [
      "Find your next adventure.",
      "Independent worlds. One place to play.",
      "Explore the collection",
    ],
    library: [
      "Your games. Ready when you are.",
      "Installed games stay playable offline.",
      "Your library",
    ],
    updates: [
      "Keep the good times current.",
      "Update your games here, with your saves kept in place.",
      "Available updates",
    ],
    favorites: [
      "The ones you keep coming back to.",
      "Your favorite worlds, all together.",
      "Your favorites",
    ],
  };
  const v = views[options.view];
  $("#heading").textContent = v[0];
  $("#subtitle").textContent = v[1];
  $("#collection-title").textContent = v[2];
  document
    .querySelectorAll("[data-view]")
    .forEach((b) =>
      b.classList.toggle("selected", b.dataset.view === options.view),
    );
  $("#feature").hidden = options.view !== "discover" || !!options.query;
  const f = games.find((g) => g.slug === "embertrail") || games[0];
  if (f)
    $("#feature").innerHTML =
      `<img src="${img(f.wiki.gallery[1]?.image || f.image)}" alt="${esc(f.name)} gameplay"><div class="feature-shade"></div><div class="feature-copy"><p class="eyebrow">STEP INTO SOMETHING NEW</p><h2>${esc(f.name)}</h2><p>${esc(f.wiki.overview)}</p><div class="badges">${badges(f)}</div><div class="feature-actions">${primary(f)}<button data-guide="${esc(f.slug)}">Explore the game →</button></div></div><span class="feature-note">REAL GAMEPLAY · FREE TO PLAY</span>`;
  if (options.view === "updates" && filtered.length) {
    $("#collection-title").innerHTML =
      'Available updates <button id="update-all">Update all</button>';
    $("#update-all").onclick = () =>
      filtered
        .filter((g) => state(g) === "update")
        .forEach((g) => enqueue(g.slug));
  }
  if (guideSlug) renderGuide(getGame(guideSlug));
}
function paragraphs(s) {
  return String(s || "")
    .split("\n\n")
    .filter(Boolean)
    .map((p) => `<p>${esc(p)}</p>`)
    .join("");
}
function renderGuide(g) {
  if (!g) return;
  const w = g.wiki;
  $("#guide-content").innerHTML =
    `<div class="guide-hero"><img src="${img(g.image)}" alt="${esc(g.name)} gameplay"><div><p class="eyebrow">GAME GUIDE</p><h1>${esc(g.name)}</h1><div class="badges">${badges(g)}</div><div class="guide-actions">${primary(g)}<span>${size(g.package.size)} · v${esc(g.version)} · Windows x64</span></div></div></div><nav class="guide-nav">${["Overview", "Rules", "Controls", "Story", "Characters", "Gallery", "Creation"].map((n) => `<a href="#guide-${n.toLowerCase()}">${n}</a>`).join("")}</nav><div class="guide-body"><section id="guide-overview"><h2>Overview</h2>${paragraphs(w.overview)}</section><div class="guide-columns"><section id="guide-rules"><h2>How to play</h2><ol>${w.rules.map((r) => `<li>${esc(r)}</li>`).join("")}</ol></section><section id="guide-controls"><h2>Controls</h2><ul>${w.controls.map((r) => `<li>${esc(r)}</li>`).join("")}</ul></section></div><section id="guide-story"><h2>The story</h2>${paragraphs(w.story)}</section><section id="guide-characters"><h2>Characters</h2>${paragraphs(w.characters_note)}<div class="character-grid">${w.characters.map((c) => `<article>${c.image ? `<img src="${img(c.image)}" alt="${esc(c.name)} in-game screenshot" loading="lazy">` : ""}<div><h3>${esc(c.name)}</h3>${paragraphs(c.description)}${paragraphs(c.appearance_note)}${c.image_caption ? `<small>${esc(c.image_caption)}</small>` : ""}${!c.image && c.image_status !== "not-rendered" ? "<small>Character screenshot not yet captured.</small>" : ""}</div></article>`).join("")}</div></section><section id="guide-gallery"><h2>Inside the game</h2><div class="gallery">${w.gallery.map((p) => `<figure><img src="${img(p.image)}" alt="${esc(p.alt)}" loading="lazy"><figcaption>${esc(p.caption)}</figcaption></figure>`).join("")}</div></section><section id="guide-creation"><h2>${g.origin === "ai" ? "Why this game was chosen" : "The original game idea"}</h2>${g.origin === "ai" ? paragraphs(w.design.rationale || "Original selection rationale not yet recovered.") : `<pre>${esc(w.design.original_prompt || "Original prompt not yet recovered")}</pre>`}${paragraphs(w.design.rationale_source)}${paragraphs(w.design.prompt_note)}</section>${installs[g.slug]?.installed ? `<section class="manage"><h2>Manage installation</h2><p>${esc(installs[g.slug].path)}</p><button class="danger" data-uninstall="${esc(g.slug)}" ${active ? "disabled" : ""}>Uninstall game</button><small>Saves and player-created files are kept.</small></section>` : ""}</div>`;
}
let lastConnection = null;
function apply(s) {
  if (s.message || lastConnection === null || s.fresh)
    lastConnection = { offline: s.offline, message: s.message };
  s = { ...s, ...lastConnection };
  games = s.catalog.games;
  installs = Object.fromEntries(s.installed.map((i) => [i.slug, i]));
  $("#connection").textContent = s.offline
    ? "Offline · saved library"
    : "Catalog up to date";
  $("#status-dot").classList.toggle("offline", s.offline);
  $("#install-folder").textContent = s.root;
  render();
  if (s.message) toast(s.message);
}
async function sync(refresh = false) {
  try {
    if (native)
      apply({
        ...(await invoke(refresh ? "refresh" : "snapshot")),
        fresh: refresh,
      });
    else {
      const c = await (await fetch("./catalog.json")).json();
      apply({
        catalog: c,
        installed: [],
        offline: false,
        root: "Windows app preview",
      });
    }
  } catch (e) {
    toast(String(e));
  }
}
async function enqueue(slug) {
  if (!native) {
    toast("Install and play are available in the Windows app.");
    return;
  }
  if (active === slug || queue.includes(slug)) return;
  queue.push(slug);
  render();
  await next();
}
async function next() {
  if (active || !queue.length) return;
  active = queue.shift();
  try {
    await invoke("install_game", { slug: active });
    render();
  } catch (e) {
    active = null;
    toast(String(e));
    render();
    await next();
  }
}
async function action(slug) {
  const g = getGame(slug);
  if (!g) return;
  if (["available", "update"].includes(state(g))) return enqueue(slug);
  if (!native) return toast("Play is available in the Windows app.");
  await play(slug);
}
async function play(slug) {
  try {
    await invoke("launch", { slug });
    await sync();
  } catch (e) {
    toast(String(e));
  }
}
document.addEventListener("click", (e) => {
  const b = e.target.closest("button");
  if (!b) return;
  if (b.dataset.action) action(b.dataset.action);
  if (b.dataset.play) play(b.dataset.play);
  if (b.dataset.guide) {
    guideSlug = b.dataset.guide;
    renderGuide(getGame(guideSlug));
    $("#guide").showModal();
  }
  if (b.dataset.favorite) {
    const s = b.dataset.favorite;
    options.favorites = options.favorites.includes(s)
      ? options.favorites.filter((x) => x !== s)
      : [...options.favorites, s];
    localStorage.setItem("favorites", JSON.stringify(options.favorites));
    render();
  }
  if (b.dataset.view) {
    options.view = b.dataset.view;
    render();
  }
  if (b.hasAttribute("data-format")) {
    options.format = b.dataset.format;
    document
      .querySelectorAll("[data-format]")
      .forEach((n) => n.classList.toggle("selected", n === b));
    render();
  }
  if (b.dataset.uninstall) {
    uninstallSlug = b.dataset.uninstall;
    $("#confirm-copy").textContent =
      `Remove ${getGame(uninstallSlug).name} from this computer? Your saves and player-created files will be kept.`;
    $("#confirmation").showModal();
  }
});
$("#search").oninput = (e) => {
  options.query = e.target.value;
  render();
};
$("#origin").onchange = (e) => {
  options.origin = e.target.value;
  render();
};
$("#sort").onchange = (e) => {
  options.sort = e.target.value;
  render();
};
$("#refresh").onclick = () => sync(true);
$("#close-guide").onclick = () => {
  $("#guide").close();
  guideSlug = null;
};
$("#guide").addEventListener("close", () => (guideSlug = null));
$("#keep").onclick = () => $("#confirmation").close();
$("#confirm-uninstall").onclick = async () => {
  $("#confirmation").close();
  try {
    await invoke("uninstall_game", { slug: uninstallSlug });
    await sync();
    toast("Game removed. Your saves were kept.");
  } catch (e) {
    toast(String(e));
  }
};
$("#cancel").onclick = () => {
  queue = [];
  if (native) invoke("cancel_download");
};
$("#app-update").onclick = async () => {
  if (!native)
    return toast("Launcher updates are available in the Windows app.");
  if (updaterBusy) return;
  if (active)
    return toast("Finish your game download before updating the launcher.");
  updaterBusy = true;
  $("#app-update").disabled = true;
  try {
    if (pendingUpdate) {
      toast(`Installing launcher ${pendingUpdate.version}…`);
      await pendingUpdate.downloadAndInstall();
      await relaunch();
    } else {
      pendingUpdate = await check();
      if (!pendingUpdate) toast("Your launcher is up to date.");
      else {
        $("#app-update").textContent =
          `Install launcher ${pendingUpdate.version}`;
        toast(
          "A launcher update is ready. Install it when you are ready to restart.",
        );
      }
    }
  } catch (e) {
    toast(`Could not update launcher: ${e}`);
  } finally {
    updaterBusy = false;
    $("#app-update").disabled = false;
  }
};
async function start() {
  try {
    imageMap = await (await fetch("./images.json")).json();
  } catch {}
  if (native) {
    await listen("download", async ({ payload: p }) => {
      $("#download-bar").hidden = false;
      $("#download-title").textContent =
        `${getGame(p.slug)?.name || p.slug}${queue.length ? ` · ${queue.length} queued` : ""}`;
      $("#download-detail").textContent =
        p.phase === "downloading"
          ? `${size(p.received)} of ${size(p.total)}`
          : p.phase === "installing"
            ? "Installing verified game…"
            : p.message;
      $("#download-progress").value = p.total
        ? (p.received / p.total) * 100
        : 0;
      $("#cancel").disabled = p.phase === "installing";
      if (["complete", "error"].includes(p.phase)) {
        active = null;
        toast(p.message);
        await sync();
        await next();
        if (!active) $("#download-bar").hidden = true;
      }
    });
    await sync();
    await sync(true);
  } else await sync();
  setInterval(() => {
    if (!active && !document.querySelector("dialog[open]")) sync();
  }, 10000);
  setInterval(
    () => {
      if (native && !active) sync(true);
    },
    15 * 60 * 1000,
  );
}
start();
