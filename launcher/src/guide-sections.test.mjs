import test from "node:test";
import assert from "node:assert/strict";
import { guideNavigation, optionalGuideSections } from "./guide-sections.mjs";
const esc = (s) =>
  String(s ?? "").replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ],
  );
const helpers = {
  esc,
  img: (value) => value,
  paragraphs: (value) =>
    String(value || "")
      .split("\n\n")
      .filter(Boolean)
      .map((p) => `<p>${esc(p)}</p>`)
      .join(""),
};
test("an abstract Scatterlock-style guide has no empty story/character sections or navigation", () => {
  const wiki = {
    overview: "Move cargo through a 3D chamber.",
    rules: ["Dock every cube."],
    controls: ["Space pulses."],
  };
  assert.deepEqual(optionalGuideSections(wiki, helpers), {
    story: "",
    characters: "",
  });
  assert.ok(!guideNavigation(wiki).includes("Story"));
  assert.ok(!guideNavigation(wiki).includes("Characters"));
  assert.ok(guideNavigation(wiki).includes("Rules"));
  assert.deepEqual(
    optionalGuideSections({ ...wiki, story: null, characters: [] }, helpers),
    { story: "", characters: "" },
  );
});
test("authored story, named characters and historical absence notes retain escaped rendering", () => {
  const wiki = {
    story: "<script>story</script>",
    characters: [
      {
        name: "Mote <img>",
        description: "A light in the world.",
        image: "mote.png",
        image_caption: "Real gameplay",
      },
    ],
  };
  const result = optionalGuideSections(wiki, helpers);
  assert.ok(guideNavigation(wiki).includes("Story"));
  assert.ok(guideNavigation(wiki).includes("Characters"));
  assert.match(result.story, /&lt;script&gt;/);
  assert.match(result.characters, /&lt;img&gt;/);
  assert.match(result.characters, /mote.png/);
  assert.doesNotMatch(result.story + result.characters, /<script>/);
  const historical = {
    characters: [],
    characters_note: "Blocks have no narrative roles.",
  };
  assert.match(
    optionalGuideSections(historical, helpers).characters,
    /Blocks have no narrative roles/,
  );
  assert.ok(guideNavigation(historical).includes("Characters"));
});
