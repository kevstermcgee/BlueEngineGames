// Plan against the complete library, independently of the current browsing view.
import { hasUpdate } from "./model.mjs";
export function planLibraryUpdates(
  games,
  installs,
  { active = null, queue = [] } = {},
) {
  const pending = new Set([active, ...queue]);
  const plan = { ready: [], running: [], queued: [] };
  for (const game of games) {
    const receipt = installs[game.slug];
    if (!hasUpdate(game, receipt)) continue;
    const category = receipt.running
      ? "running"
      : pending.has(game.slug)
        ? "queued"
        : "ready";
    plan[category].push(game);
  }
  return plan;
}
