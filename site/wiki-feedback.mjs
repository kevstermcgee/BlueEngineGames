import {
  renderCollaboration,
  mountCollaboration,
  mountGlobalGameDrop,
} from "./collaboration.mjs";
import {
  mountFeedback,
  renderFeedback,
  mountPublicScores,
} from "./feedback.mjs";

const root = document.querySelector("[data-feedback-game]");
if (root) {
  const game = JSON.parse(root.dataset.feedbackGame);
  root.innerHTML = renderFeedback(game);
  mountFeedback(root, game);
}

void mountPublicScores(document);

let globalImporter;
for (const element of document.querySelectorAll("[data-community-root]")) {
  const game = element.dataset.communityGame
    ? JSON.parse(element.dataset.communityGame)
    : null;
  element.innerHTML = renderCollaboration(game, {
    sectionId: game ? "guide-collaborate" : "creator-collaborate",
  });
  const importer = mountCollaboration(element, game);
  if (!globalImporter) {
    globalImporter = importer;
    mountGlobalGameDrop(document, importer.importFile, () => {
      const details = element.closest("details");
      if (details) details.open = true;
      element.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  }
}
