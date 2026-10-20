// Story and named characters are optional facts in the shared game wiki.
const hasCharacters = (wiki) =>
  !!(wiki.characters?.length || wiki.characters_note);
export function guideNavigation(wiki) {
  return [
    "Overview",
    "Versions",
    "Rules",
    "Controls",
    ...(wiki.story ? ["Story"] : []),
    ...(hasCharacters(wiki) ? ["Characters"] : []),
    "Gallery",
    "Creation",
    "Feedback",
    "Collaborate",
  ];
}
export function optionalGuideSections(w, { esc, paragraphs, img }) {
  return {
    story: w.story
      ? `<section id="guide-story"><h2>The story</h2>${paragraphs(w.story)}</section>`
      : "",
    characters: hasCharacters(w)
      ? `<section id="guide-characters"><h2>Characters</h2>${paragraphs(w.characters_note)}<div class="character-grid">${(w.characters || []).map((c) => `<article>${c.image ? `<img src="${img(c.image)}" alt="${esc(c.name)} in-game screenshot" loading="lazy">` : ""}<div><h3>${esc(c.name)}</h3>${paragraphs(c.description)}${paragraphs(c.appearance_note)}${c.image_caption ? `<small>${esc(c.image_caption)}</small>` : ""}${!c.image && c.image_status !== "not-rendered" ? "<small>Character screenshot not yet captured.</small>" : ""}</div></article>`).join("")}</div></section>`
      : "",
  };
}
