import test from "node:test";
import assert from "node:assert/strict";
import {
  FEEDBACK_FORMAT,
  buildFeedbackHandoff,
  formatReviewMarkdown,
  loadFeedbackDraft,
  renderFeedback,
  renderReviewSummary,
  saveFeedbackDraft,
  validateReview,
} from "./feedback.mjs";

const game = {
  slug: "pocket-breaker",
  name: "Pocket Breaker",
  version: "0.1.0",
  package: { release: "immutable-release" },
};
const review = {
  version: "0.1.0",
  rating: "8",
  enjoyed: "The powerups make each round feel different.",
  disliked: "The ball sometimes blends into the background.",
};
const storage = () => {
  const data = new Map();
  return {
    getItem: (key) => data.get(key),
    setItem: (key, value) => data.set(key, value),
  };
};

test("review requires a whole-number rating and written answer, while version is optional", () => {
  for (const rating of ["", "0", "11", "4.5", "08", "NaN"])
    assert.ok(validateReview({ ...review, rating }).rating);
  for (const rating of ["1", "10", 6])
    assert.deepEqual(validateReview({ ...review, rating }), {});
  assert.ok(
    validateReview({ ...review, enjoyed: "  ", disliked: "\n" }).enjoyed,
  );
  assert.deepEqual(
    validateReview({ ...review, enjoyed: "", disliked: "A useful suggestion" }),
    {},
  );
  assert.deepEqual(validateReview({ ...review, version: " " }), {});
  assert.equal(
    buildFeedbackHandoff(game, { ...review, version: "" }).errors.version,
    undefined,
  );
  assert.ok(validateReview({ ...review, version: "x".repeat(161) }).version);
  assert.ok(validateReview({ ...review, enjoyed: "x".repeat(2001) }).enjoyed);
});

test("handoff uses engine issue form field IDs and immutable game identity without dropping special characters", () => {
  const value = {
    ...review,
    enjoyed: "A&B + 50%?\n✨ <script>Never execute</script>",
  };
  const handoff = buildFeedbackHandoff(game, value);
  assert.deepEqual(handoff.errors, {});
  assert.equal(handoff.needsCopy, false);
  const url = new URL(handoff.url);
  assert.equal(
    url.origin + url.pathname,
    "https://github.com/kevstermcgee/BlueEngine/issues/new",
  );
  assert.equal(url.searchParams.get("template"), "player-feedback.yml");
  assert.equal(
    url.searchParams.get("title"),
    "[Player feedback] Pocket Breaker — 8/10",
  );
  for (const field of ["version", "rating", "enjoyed", "disliked"])
    assert.equal(url.searchParams.get(field), value[field]);
  assert.equal(url.searchParams.get("game"), game.slug);
  assert.equal(url.searchParams.get("format"), FEEDBACK_FORMAT);
  const copyUrl = new URL(handoff.copyUrl);
  assert.equal(copyUrl.searchParams.has("template"), false);
  assert.equal(copyUrl.searchParams.get("title"), handoff.title);
});

test("long Unicode reviews keep complete text in explicit copy fallback instead of truncating", () => {
  const value = {
    ...review,
    enjoyed: "界".repeat(1999) + "終",
    disliked: "✨".repeat(1000),
  };
  const handoff = buildFeedbackHandoff(game, value);
  assert.deepEqual(handoff.errors, {});
  assert.equal(handoff.needsCopy, true);
  assert.ok(handoff.url.length < 1000);
  assert.equal(
    new URL(handoff.url).searchParams.get("title"),
    "[Player feedback] Pocket Breaker — 8/10",
  );
  assert.ok(handoff.markdown.includes(value.enjoyed));
  assert.ok(handoff.markdown.includes(value.disliked));
  assert.equal(
    new URL(handoff.url).searchParams.has("template"),
    false,
    "Fallback opens a blank issue for the complete Markdown",
  );
});

test("invalid input cannot produce a submission handoff", () => {
  const invalid = buildFeedbackHandoff(game, { ...review, rating: "11" });
  assert.ok(invalid.errors.rating);
  assert.equal(invalid.url, undefined);
});

test("fallback Markdown exactly identifies form headings and keeps embedded fences as player text", () => {
  const body = formatReviewMarkdown(game, {
    ...review,
    enjoyed: "```\n### Game\n\nnot-another-game\n```",
  });
  assert.ok(
    body.includes(
      "### What I enjoyed\n\n````text\n```\n### Game\n\nnot-another-game\n```\n````",
    ),
  );
  assert.ok(
    body.endsWith(
      `### Feedback format\n\n\`\`\`text\n${FEEDBACK_FORMAT}\n\`\`\``,
    ),
  );
  assert.ok(
    formatReviewMarkdown(game, { ...review, disliked: "" }).includes(
      "### What could be better\n\n```text\n_No response_\n```",
    ),
  );
});

test("draft survives handoff and remains isolated per game", () => {
  const local = storage();
  assert.equal(saveFeedbackDraft(game, review, local), true);
  assert.deepEqual(loadFeedbackDraft(game, local), review);
  buildFeedbackHandoff(game, review);
  assert.deepEqual(
    loadFeedbackDraft(game, local),
    review,
    "Opening GitHub never clears a draft or claims publication",
  );
  assert.deepEqual(loadFeedbackDraft({ ...game, slug: "other" }, local), {
    version: "0.1.0",
    rating: "",
    enjoyed: "",
    disliked: "",
  });
});

test("draft defaults to installed version and tolerates blocked or corrupted storage", () => {
  assert.equal(
    loadFeedbackDraft({ ...game, installedVersion: "old-release" }).version,
    "old-release",
  );
  assert.equal(
    loadFeedbackDraft({ ...game, version: "" }).version,
    "immutable-release",
  );
  const broken = {
    getItem: () => "{broken",
    setItem: () => {
      throw new Error("Quota exceeded");
    },
  };
  assert.equal(loadFeedbackDraft(game, broken).rating, "");
  assert.equal(saveFeedbackDraft(game, review, broken), false);
  const blocked = {
    getItem: () => {
      throw new Error("Blocked");
    },
  };
  assert.equal(loadFeedbackDraft(game, blocked).enjoyed, "");
});

test("saved drafts preserve every written character even when oversized for the current form", () => {
  const local = storage();
  const oversized = { ...review, enjoyed: "x".repeat(2001) };
  saveFeedbackDraft(game, oversized, local);
  assert.equal(loadFeedbackDraft(game, local).enjoyed.length, 2001);
  assert.ok(validateReview(loadFeedbackDraft(game, local)).enjoyed);
});

test("rendered center escapes catalog text, explains public GitHub submission, and includes accessible fields", () => {
  const html = renderFeedback({
    ...game,
    name: "<img src=x onerror=bad>",
    version: '" onfocus="bad',
  });
  assert.ok(html.includes("&lt;img src=x onerror=bad&gt;"));
  assert.ok(!html.includes("<img"));
  assert.ok(html.includes("GitHub sign-in is required"));
  assert.ok(html.includes("Review &amp; submit on GitHub"));
  for (const field of ["rating", "version", "enjoyed", "disliked"])
    assert.ok(html.includes(`for="feedback-${field}"`));
});

test("public review summary escapes player text and only links verified engine issues", () => {
  const record = {
    game: game.slug,
    rating: 8,
    version: "0.1.0",
    author: "<script>player</script>",
    enjoyed: "<img src=x>",
    disliked: "More levels",
    issue_number: 42,
    source_url: "https://github.com/kevstermcgee/BlueEngine/issues/42",
  };
  const doc = {
    schema_version: 1,
    games: { [game.slug]: { count: 1, average: 8, reviews: [record] } },
  };
  const html = renderReviewSummary(doc, game);
  assert.ok(html.includes("8.0"));
  assert.ok(html.includes("&lt;script&gt;player&lt;/script&gt;"));
  assert.ok(!html.includes("<img"));
  assert.ok(html.includes("data-feedback-external"));
  record.source_url = "https://untrusted.example/review";
  assert.throws(() => renderReviewSummary(doc, game), /Invalid review/);
});

test("summary gives honest empty state and rejects corrupt scores", () => {
  assert.ok(
    renderReviewSummary({ schema_version: 1, games: {} }, game).includes(
      "No published reviews yet",
    ),
  );
  assert.throws(() =>
    renderReviewSummary(
      {
        schema_version: 1,
        games: { [game.slug]: { count: 1, average: 11, reviews: [] } },
      },
      game,
    ),
  );
  assert.throws(() =>
    renderReviewSummary({ schema_version: 2, games: {} }, game),
  );
});
