import { mountFeedback, renderFeedback } from "./feedback.mjs";

const root = document.querySelector("[data-feedback-game]");
if (root) {
  const game = JSON.parse(root.dataset.feedbackGame);
  root.innerHTML = renderFeedback(game);
  mountFeedback(root, game);
}
