import { renderCollaboration, mountCollaboration } from "./collaboration.mjs";
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

for (const element of document.querySelectorAll("[data-community-root]")) {
  const game = element.dataset.communityGame
    ? JSON.parse(element.dataset.communityGame)
    : null;
  element.innerHTML = renderCollaboration(game, {
    sectionId: game ? "guide-collaborate" : "creator-collaborate",
  });
  mountCollaboration(element, game);
}
