import test from "node:test";
import assert from "node:assert/strict";
import {
  releaseDetails,
  releaseDate,
  installationDate,
  versionLabel,
} from "./game-versions.mjs";

const game = {
  version: "0.1.0",
  created: "2020-01-01",
  released_at: "2026-10-18T01:00:00Z",
  package: { release: "build-2" },
  versions: [
    {
      release: "build-2",
      name: "Second build",
      released_at: "2026-10-18T01:00:00Z",
    },
    {
      release: "build-1",
      name: "First build",
      released_at: "2026-10-15T23:00:00Z",
    },
  ],
};
test("an uninstalled game's header describes the latest release", () => {
  const details = releaseDetails(game);
  assert.equal(details.installed, null);
  assert.equal(details.latest.name, "0.1.0");
  assert.equal(details.latest.released_at, game.released_at);
  assert.deepEqual(
    details.versions.map((v) => v.release),
    ["build-2", "build-1"],
  );
});
test("legacy installs match their receipt to history, never the latest semantic version", () => {
  const { installed } = releaseDetails(game, {
    installed: true,
    release: "build-1",
  });
  assert.equal(installed.name, "First build");
  assert.equal(installed.released_at, game.versions[1].released_at);
  assert.equal(installed.installed_at, null);
});
test("saved install metadata survives a release leaving the feed", () => {
  const receipt = {
    installed: true,
    release: "old-build",
    version: "0.0.9",
    released_at: "2026-10-01T12:00:00Z",
    installed_at: 1790848800000,
  };
  const { installed } = releaseDetails(game, receipt);
  assert.equal(installed.name, "0.0.9");
  assert.equal(installed.released_at, receipt.released_at);
  assert.equal(installed.installed_at, receipt.installed_at);
});
test("missing legacy dates stay unknown rather than borrowing creation or latest dates", () => {
  const { installed } = releaseDetails(game, {
    installed: true,
    release: "unlisted",
  });
  assert.equal(installed.name, "unlisted");
  assert.equal(installed.released_at, null);
  const oldFeed = releaseDetails({
    ...game,
    versions: undefined,
    released_at: undefined,
  });
  assert.equal(oldFeed.latest.released_at, null);
  assert.equal(oldFeed.versions.length, 1);
});
test("catalog placeholders use a real published release identity", () => {
  const details = releaseDetails(
    { ...game, version: "catalog" },
    {
      installed: true,
      release: "build-2",
      version: "catalog",
    },
  );
  assert.equal(details.latest.name, "Second build");
  assert.equal(details.installed.name, "Second build");
});
test("date labels reject missing/invalid data and release days use UTC", () => {
  assert.equal(releaseDate(undefined), null);
  assert.equal(releaseDate("invalid"), null);
  assert.equal(installationDate(null), null);
  assert.equal(installationDate("2026-10-18"), null);
  assert.equal(installationDate(Infinity), null);
  assert.equal(
    releaseDate("2026-10-18T01:00:00Z"),
    new Intl.DateTimeFormat(undefined, {
      dateStyle: "medium",
      timeZone: "UTC",
    }).format(new Date("2026-10-18T01:00:00Z")),
  );
  assert.equal(versionLabel("0.1.0"), "v0.1.0");
  assert.equal(versionLabel("build-2"), "build-2");
});
