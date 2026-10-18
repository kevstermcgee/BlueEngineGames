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
  loadPublicReviewSummary,
  readReviewCollection,
  renderPublicScore,
  renderReviewPage,
  reviewDataUrl,
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

const publishedReview = (issueNumber, rating = 8) => ({
  game: game.slug,
  rating,
  version: "0.1.0",
  author: `player-${issueNumber}`,
  enjoyed: "A clever mechanic",
  disliked: "More levels",
  issue_number: issueNumber,
  source_url: `https://github.com/kevstermcgee/BlueEngine/issues/${issueNumber}`,
});

test("public badges distinguish no reviews, loading, offline and real player scores", () => {
  assert.match(renderPublicScore(game), /Loading player scores/);
  assert.match(
    renderPublicScore(game, { unavailable: true }),
    /Reviews unavailable/,
  );
  const document = { schema_version: 1, games: {} };
  assert.match(renderPublicScore(game, { document }), /No reviews yet/);
  document.games[game.slug] = {
    count: 7,
    average: 8.4,
    reviews: [publishedReview(7)],
  };
  assert.match(renderPublicScore(game, { document }), /8.4.*7 reviews/s);
  document.games[game.slug].average = 12;
  assert.match(renderPublicScore(game, { document }), /Reviews unavailable/);
});

test("summary keeps a public in-wiki path to every review beyond the recent three", () => {
  const document = {
    schema_version: 1,
    games: {
      [game.slug]: {
        count: 8,
        average: 8,
        reviews: [publishedReview(8), publishedReview(7), publishedReview(6)],
      },
    },
  };
  const html = renderReviewSummary(document, game);
  assert.match(html, /Browse all 8 reviews/);
  assert.match(html, /data-feedback-browse/);
  assert.equal((html.match(/class="feedback-review"/g) || []).length, 3);
  assert.match(
    renderFeedback(game),
    /Read published reviews here without signing in/,
  );
});

test("all-review collection paginates complete writeups without auth, duplicates or cross-game records", () => {
  const document = {
    schema_version: 1,
    game: game.slug,
    count: 8,
    average: 8,
    reviews: Array.from({ length: 8 }, (_, i) => publishedReview(8 - i)),
  };
  const first = renderReviewPage(document, game);
  const last = renderReviewPage(document, game, { page: 1 });
  assert.match(first, /Showing 1–6 of 8/);
  assert.match(last, /Showing 7–8 of 8/);
  assert.equal((first.match(/class="feedback-review"/g) || []).length, 6);
  assert.equal((last.match(/class="feedback-review"/g) || []).length, 2);
  assert.match(last, /player-1/);
  assert.doesNotMatch(first, /player-1/);
  assert.throws(
    () => renderReviewPage(document, game, { page: 2 }),
    /Invalid review page/,
  );
  assert.throws(
    () => readReviewCollection({ ...document, count: 9 }, game),
    /Invalid review collection/,
  );
  assert.throws(
    () => readReviewCollection({ ...document, game: "other" }, game),
    /Invalid review collection/,
  );
  assert.throws(
    () =>
      readReviewCollection(
        { ...document, reviews: Array(8).fill(publishedReview(1)) },
        game,
      ),
    /Invalid review collection/,
  );
  assert.throws(
    () =>
      readReviewCollection(
        {
          ...document,
          reviews: document.reviews.map((r) => ({
            ...r,
            source_url: "javascript:alert(1)",
          })),
        },
        game,
      ),
    /Invalid review collection/,
  );
});

test("all-review pages escape player content and reject malformed empty feeds", () => {
  const document = {
    schema_version: 1,
    game: game.slug,
    count: 1,
    average: 8,
    reviews: [
      {
        ...publishedReview(1),
        enjoyed: '<img src=x onerror="bad()">',
        author: "<script>x</script>",
      },
    ],
  };
  const html = renderReviewPage(document, game);
  assert.ok(!html.includes("<img"));
  assert.match(html, /&lt;script&gt;x&lt;\/script&gt;/);
  assert.match(
    renderReviewPage(
      { ...document, count: 0, average: null, reviews: [] },
      game,
    ),
    /No published reviews/,
  );
  assert.throws(
    () => readReviewCollection({ ...document, count: 0 }, game),
    /Invalid review summary/,
  );
  assert.equal(
    reviewDataUrl(game),
    "https://raw.githubusercontent.com/kevstermcgee/BlueEngine/main/docs/player-feedback/games/pocket-breaker.json",
  );
  assert.throws(
    () => reviewDataUrl({ slug: "../summary" }),
    /Invalid game identity/,
  );
});

test("public summary coalesces simultaneous card/wiki reads, refreshes explicitly and retries failures", async () => {
  let calls = 0;
  const document = { schema_version: 1, games: {} };
  const fetcher = async (url, options) => {
    calls++;
    assert.equal(url.endsWith("/summary.json"), true);
    assert.equal(options.credentials, "omit");
    return { ok: true, json: async () => document };
  };
  const a = loadPublicReviewSummary({ fetcher });
  const b = loadPublicReviewSummary({ fetcher });
  assert.equal(a, b);
  assert.equal(await a, document);
  assert.equal(await loadPublicReviewSummary({ fetcher }), document);
  assert.equal(calls, 1);
  await loadPublicReviewSummary({ fetcher, force: true });
  assert.equal(calls, 2);
  let retry = 0;
  const flaky = async () => ({ ok: ++retry > 1, json: async () => document });
  await assert.rejects(
    loadPublicReviewSummary({ fetcher: flaky }),
    /Reviews unavailable/,
  );
  assert.equal(await loadPublicReviewSummary({ fetcher: flaky }), document);
});
