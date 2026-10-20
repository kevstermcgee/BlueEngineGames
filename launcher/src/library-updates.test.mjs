import test from "node:test";
import assert from "node:assert/strict";
import { planLibraryUpdates } from "./library-updates.mjs";
import { filterGames } from "./model.mjs";

const game = (slug, extra = {}) => ({
  slug,
  name: slug,
  description: "Explore a world",
  created: "2026-10-17",
  version: "0.1.0",
  origin: "human",
  presentation: "2d",
  package: { release: "immutable-build-2", size: 20 },
  wiki: {
    overview: "A journey",
    story: "Lost city",
    rules: ["Reach the beacon"],
    characters: [{ name: "Mote" }],
  },
  ...extra,
});
const outdated = (extra = {}) => ({
  installed: true,
  release: "immutable-build-1",
  version: "0.1.0",
  ...extra,
});
const slugs = (plan) =>
  Object.fromEntries(
    Object.entries(plan).map(([category, games]) => [
      category,
      games.map((g) => g.slug),
    ]),
  );

test("only installed games with a different immutable release need updates", () => {
  const games = [
    "same-version-update",
    "current",
    "not-owned",
    "removed",
    "truthy-flag",
    "old-receipt",
  ].map((slug) => game(slug));
  const installs = {
    "same-version-update": outdated(),
    current: outdated({ release: "immutable-build-2", version: "0.0.1" }),
    removed: outdated({ installed: false }),
    "truthy-flag": outdated({ installed: "true" }),
    "old-receipt": { installed: true },
  };
  assert.deepEqual(slugs(planLibraryUpdates(games, installs)), {
    ready: ["same-version-update", "old-receipt"],
    running: [],
    queued: [],
  });
});

test("bulk planning includes installed updates hidden by search, designer, format and favorites filters", () => {
  const games = [
    game("visible"),
    game("hidden-ai", { origin: "ai" }),
    game("hidden-3d", { presentation: "3d" }),
    game("unowned", { origin: "ai" }),
  ];
  const installs = Object.fromEntries(
    games.slice(0, 3).map((g) => [g.slug, outdated()]),
  );
  const visible = filterGames(games, installs, {
    view: "favorites",
    favorites: ["visible"],
    query: "visible",
    origin: "human",
    format: "2d",
  });
  assert.deepEqual(
    visible.map((g) => g.slug),
    ["visible"],
  );
  assert.deepEqual(slugs(planLibraryUpdates(games, installs)), {
    ready: ["visible", "hidden-ai", "hidden-3d"],
    running: [],
    queued: [],
  });
});

test("running updates stay deferred even if active or queued, while pending downloads are never ready twice", () => {
  const games = [
    "ready",
    "active",
    "queued",
    "running-active",
    "running-queued",
    "current-running",
    "unowned-running",
  ].map((slug) => game(slug));
  const installs = Object.fromEntries(games.map((g) => [g.slug, outdated()]));
  installs["running-active"].running = true;
  installs["running-queued"].running = true;
  installs["current-running"] = outdated({
    running: true,
    release: "immutable-build-2",
  });
  installs["unowned-running"] = outdated({ running: true, installed: false });
  const plan = planLibraryUpdates(games, installs, {
    active: "active",
    queue: [
      "queued",
      "queued",
      "active",
      "running-queued",
      "unknown",
      "unowned-running",
    ],
  });
  assert.deepEqual(slugs(plan), {
    ready: ["ready"],
    running: ["running-active", "running-queued"],
    queued: ["active", "queued"],
  });
  const runningWhileActive = planLibraryUpdates(games, installs, {
    active: "running-active",
  });
  assert.ok(runningWhileActive.running.includes(games[3]));
  assert.ok(!runningWhileActive.queued.includes(games[3]));
});

test("when a game stops or its download finishes, a fresh plan reflects its current receipt", () => {
  const g = game("game");
  const installs = { game: outdated({ running: true }) };
  assert.deepEqual(slugs(planLibraryUpdates([g], installs)), {
    ready: [],
    running: ["game"],
    queued: [],
  });
  installs.game.running = false;
  assert.deepEqual(
    slugs(planLibraryUpdates([g], installs, { queue: ["game"] })),
    { ready: [], running: [], queued: ["game"] },
  );
  assert.deepEqual(slugs(planLibraryUpdates([g], installs)), {
    ready: ["game"],
    running: [],
    queued: [],
  });
  installs.game.release = g.package.release;
  assert.deepEqual(planLibraryUpdates([g], installs), {
    ready: [],
    running: [],
    queued: [],
  });
});

test("planning preserves the original game objects and catalog order without mutating caller data", () => {
  const first = Object.freeze(game("z-first"));
  const second = Object.freeze(game("a-second"));
  const games = Object.freeze([first, second]);
  const installs = Object.freeze({
    "z-first": Object.freeze(outdated()),
    "a-second": Object.freeze(outdated()),
  });
  const queue = Object.freeze(["z-first", "z-first"]);
  const options = Object.freeze({ active: null, queue });
  const plan = planLibraryUpdates(games, installs, options);
  assert.equal(plan.queued[0], first);
  assert.equal(plan.ready[0], second);
  assert.deepEqual(queue, ["z-first", "z-first"]);
  assert.deepEqual(planLibraryUpdates(games, installs).ready, [first, second]);
  assert.deepEqual(planLibraryUpdates([], {}), {
    ready: [],
    running: [],
    queued: [],
  });
});
