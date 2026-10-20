import test from "node:test";
import assert from "node:assert/strict";
import { filterGames, status } from "./model.mjs";
const game = (slug, origin = "human") => ({
  slug,
  name: slug,
  description: "A world",
  created: "2026-10-17",
  origin,
  presentation: "2d",
  package: { release: "build-2", size: 20 },
  wiki: {
    overview: "A journey",
    story: "Lost city",
    rules: ["Reach the beacon"],
    characters: [{ name: "Mote" }],
  },
});
test("install receipts distinguish updates even if semantic versions match", () => {
  const g = game("first");
  assert.equal(status(g, {}), "available");
  assert.equal(status(g, { installed: true, release: "build-1" }), "update");
  assert.equal(status(g, { installed: true, release: "build-2" }), "installed");
  assert.equal(
    status(g, { installed: true, running: true, release: "build-1" }),
    "running",
  );
});
test("library, updates, designers, favorites and wiki search compose", () => {
  const games = [game("alpha"), game("beta", "ai")],
    installed = { alpha: { installed: true, release: "build-1" } };
  assert.deepEqual(
    filterGames(games, installed, { view: "updates" }).map((g) => g.slug),
    ["alpha"],
  );
  assert.equal(
    filterGames(games, installed, { origin: "ai", query: "Mote" }).length,
    1,
  );
  assert.equal(
    filterGames(games, installed, { view: "library", origin: "ai" }).length,
    0,
  );
  assert.equal(
    filterGames(games, installed, {
      view: "favorites",
      favorites: ["beta"],
      query: "beacon",
    }).length,
    1,
  );
});
test("running games with a newer release remain visible in Updates until updated", () => {
  const games = [game("old-running"), game("current-running"), game("unowned")];
  const installs = {
    "old-running": { installed: true, running: true, release: "build-1" },
    "current-running": { installed: true, running: true, release: "build-2" },
  };
  assert.deepEqual(
    filterGames(games, installs, { view: "updates" }).map((g) => g.slug),
    ["old-running"],
  );
  assert.equal(status(games[0], installs["old-running"]), "running");
});

test("abstract games remain searchable without optional story or characters", () => {
  const value = game("scatterlock", "ai");
  delete value.wiki.story;
  delete value.wiki.characters;
  assert.deepEqual(
    filterGames([value], {}, { query: "beacon" }).map((g) => g.slug),
    ["scatterlock"],
  );
  assert.deepEqual(
    filterGames([value], {}, { query: "invented-character" }),
    [],
  );
});
