import test from "node:test";
import assert from "node:assert/strict";
import {
  FEEDBACK_FORMAT,
  FEEDBACK_SUBMIT_URL,
  buildFeedbackSubmission,
  submitFeedback,
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
  mountFeedback,
} from "./feedback.mjs";

const game = {
  slug: "pocket-breaker",
  name: "Pocket Breaker",
  version: "0.1.0",
  package: { release: "immutable-release" },
};
const review = {
  name: "",
  version: "0.1.0",
  rating: "8",
  enjoyed: "The powerups make each round feel different.",
  disliked: "The ball sometimes blends into the background.",
};
const submissionId = "6d06cc45-a876-4b89-8bd2-fb67042f9472";
const storage = () => {
  const data = new Map();
  return {
    getItem: (key) => data.get(key),
    setItem: (key, value) => data.set(key, value),
    removeItem: (key) => data.delete(key),
  };
};

test("review requires a whole-number rating and written answer; name and version are optional", () => {
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
  assert.deepEqual(
    validateReview({ ...review, version: " ", name: undefined }),
    {},
  );
  assert.deepEqual(validateReview({ ...review, name: "Player" }), {});
  assert.ok(validateReview({ ...review, name: "x".repeat(81) }).name);
  assert.ok(validateReview({ ...review, name: 123 }).name);
  assert.ok(validateReview({ ...review, version: "x".repeat(161) }).version);
  assert.ok(validateReview({ ...review, enjoyed: "x".repeat(2001) }).enjoyed);
});

test("submission contains only review data, optional name and a stable submission identity", () => {
  const value = {
    ...review,
    name: "  Ada  ",
    enjoyed: "A&B + 50%?\n✨ <script>Never execute</script>",
  };
  const result = buildFeedbackSubmission(game, value, submissionId);
  assert.deepEqual(result.errors, {});
  assert.deepEqual(result.payload, {
    format: FEEDBACK_FORMAT,
    submission_id: submissionId,
    game: game.slug,
    name: "Ada",
    version: review.version,
    rating: 8,
    enjoyed: value.enjoyed,
    disliked: review.disliked,
  });
  assert.equal(
    buildFeedbackSubmission(game, review, submissionId).payload.name,
    "",
  );
  const invalid = buildFeedbackSubmission(
    game,
    { ...review, rating: "11" },
    submissionId,
  );
  assert.ok(invalid.errors.rating);
  assert.equal(invalid.payload, undefined);
  assert.throws(
    () => buildFeedbackSubmission(game, review, "not-a-uuid"),
    /Invalid submission identity/,
  );
  assert.throws(
    () => buildFeedbackSubmission(game, review, submissionId.toUpperCase()),
    /Invalid submission identity/,
  );
  assert.throws(
    () =>
      buildFeedbackSubmission(
        { ...game, slug: "../other" },
        review,
        submissionId,
      ),
    /Invalid game identity/,
  );
});

test("submission POST sends JSON without account credentials and requires exact receipt", async () => {
  const { payload } = buildFeedbackSubmission(game, review, submissionId);
  const result = await submitFeedback(payload, {
    fetcher: async (url, options) => {
      assert.equal(url, FEEDBACK_SUBMIT_URL);
      assert.equal(options.method, "POST");
      assert.equal(options.credentials, "omit");
      assert.deepEqual(options.headers, { "Content-Type": "application/json" });
      assert.deepEqual(JSON.parse(options.body), payload);
      return {
        ok: true,
        json: async () => ({ accepted: true, submission_id: submissionId }),
      };
    },
  });
  assert.equal(result.accepted, true);
  for (const response of [
    { ok: false },
    {
      ok: true,
      json: async () => ({ accepted: false, submission_id: submissionId }),
    },
    {
      ok: true,
      json: async () => ({ accepted: true, submission_id: "another-id" }),
    },
  ])
    await assert.rejects(
      submitFeedback(payload, { fetcher: async () => response }),
    );
});

test("drafts preserve optional name and retry identity, and remain isolated per game", () => {
  const local = storage();
  const draft = { ...review, name: "Ada", submission_id: submissionId };
  assert.equal(saveFeedbackDraft(game, draft, local), true);
  assert.deepEqual(loadFeedbackDraft(game, local), draft);
  assert.deepEqual(loadFeedbackDraft({ ...game, slug: "other" }, local), {
    name: "",
    submission_id: "",
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
  assert.equal(
    loadFeedbackDraft(game, {
      getItem: () => {
        throw new Error("Blocked");
      },
    }).enjoyed,
    "",
  );
  const local = storage();
  saveFeedbackDraft(
    game,
    {
      ...review,
      enjoyed: "x".repeat(2001),
      name: "x".repeat(81),
      submission_id: "bad",
    },
    local,
  );
  assert.equal(loadFeedbackDraft(game, local).enjoyed.length, 2001);
  assert.equal(loadFeedbackDraft(game, local).name.length, 81);
  assert.equal(loadFeedbackDraft(game, local).submission_id, "");
  assert.ok(validateReview(loadFeedbackDraft(game, local)).enjoyed);
});

test("rendered center escapes catalog text and offers accessible account-free submission", () => {
  const html = renderFeedback({
    ...game,
    name: "<img src=x onerror=bad>",
    version: '\" onfocus=\"bad',
  });
  assert.ok(html.includes("&lt;img src=x onerror=bad&gt;"));
  assert.ok(!html.includes("<img"));
  assert.match(html, /No account is needed/);
  assert.match(html, /Submit feedback/);
  assert.match(html, /Anonymous/);
  assert.doesNotMatch(html, /GitHub|github|issues\/new/);
  for (const field of ["name", "rating", "version", "enjoyed", "disliked"])
    assert.ok(html.includes(`for="feedback-${field}"`));
});

test("public review summary escapes legacy player text without account links", () => {
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
  assert.doesNotMatch(html, /data-feedback-external|GitHub/);
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
    renderReviewSummary({ schema_version: 3, games: {} }, game),
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

test("published review reads use the backend Unicode character limit", () => {
  const enjoyed = "🎮".repeat(2000);
  const document = {
    schema_version: 1,
    game: game.slug,
    count: 1,
    average: 8,
    reviews: [{ ...publishedReview(1), enjoyed }],
  };
  assert.ok(renderReviewPage(document, game).includes(enjoyed));
  document.reviews[0].enjoyed += "🎮";
  assert.throws(
    () => readReviewCollection(document, game),
    /Invalid review collection/,
  );
});

const newPublishedReview = (id = submissionId, name = "") => ({
  game: game.slug,
  submission_id: id,
  name,
  rating: 8,
  version: "0.1.0",
  enjoyed: "Clever mechanics",
  disliked: "",
});

test("account-free records display optional names or Anonymous alongside legacy reviews", () => {
  const collection = {
    schema_version: 2,
    game: game.slug,
    count: 3,
    average: 8,
    reviews: [
      newPublishedReview(submissionId, "<script>Player</script>"),
      newPublishedReview("5f458b5a-3162-4079-a1db-3f9bda604ae2"),
      publishedReview(1),
    ],
  };
  const html = renderReviewPage(collection, game);
  assert.match(html, /&lt;script&gt;Player&lt;\/script&gt;/);
  assert.match(html, /Anonymous/);
  assert.match(html, /player-1/);
  assert.doesNotMatch(html, /<script>|GitHub|github|href=/);
  assert.throws(
    () =>
      readReviewCollection(
        {
          ...collection,
          reviews: collection.reviews.map(() => newPublishedReview()),
        },
        game,
      ),
    /Invalid review collection/,
  );
  for (const invalid of [
    { ...newPublishedReview(), name: "x".repeat(81) },
    { ...newPublishedReview(), submission_id: "invalid" },
    { ...newPublishedReview(), rating: 12 },
  ])
    assert.throws(
      () =>
        readReviewCollection(
          { ...collection, count: 1, reviews: [invalid] },
          game,
        ),
      /Invalid review collection/,
    );
});

function fakeNode() {
  const listeners = new Map();
  const attributes = new Map();
  return {
    value: "",
    textContent: "",
    innerHTML: "",
    disabled: false,
    addEventListener(type, listener) {
      listeners.set(type, listener);
    },
    removeEventListener(type, listener) {
      if (listeners.get(type) === listener) listeners.delete(type);
    },
    async emit(type, event = {}) {
      return listeners.get(type)?.({ preventDefault() {}, ...event });
    },
    setAttribute(name, value) {
      attributes.set(name, value);
    },
    removeAttribute(name) {
      attributes.delete(name);
    },
    getAttribute(name) {
      return attributes.get(name);
    },
    focus() {
      this.focused = true;
    },
  };
}

function feedbackFixture({
  local = storage(),
  submitReview,
  value = review,
} = {}) {
  const fields = Object.fromEntries(
    ["name", "version", "rating", "enjoyed", "disliked"].map((name) => [
      name,
      fakeNode(),
    ]),
  );
  const status = fakeNode();
  const draft = fakeNode();
  const submit = fakeNode();
  const reviews = fakeNode();
  const errors = Object.fromEntries(
    Object.keys(fields).map((name) => [name, fakeNode()]),
  );
  const counts = { enjoyed: fakeNode(), disliked: fakeNode() };
  const form = fakeNode();
  form.elements = { namedItem: (name) => fields[name] };
  form.querySelector = (selector) => {
    if (selector === "[data-feedback-status]") return status;
    if (selector === "[data-feedback-draft]") return draft;
    if (selector === '[type="submit"]') return submit;
    return (
      errors[selector.match(/^\[data-feedback-error="(.+)"\]$/)?.[1]] ||
      counts[selector.match(/^\[data-feedback-count="(.+)"\]$/)?.[1]]
    );
  };
  const root = fakeNode();
  root.querySelector = (selector) =>
    selector === "[data-feedback-form]" ? form : reviews;
  const cleanup = mountFeedback(root, game, {
    storage: local,
    fetcher: null,
    submitReview,
  });
  const input = async (next) => {
    for (const [name, content] of Object.entries(next))
      fields[name].value = content;
    await form.emit("input");
  };
  if (value)
    for (const [name, content] of Object.entries(value))
      fields[name].value = content;
  return { form, fields, status, draft, submit, local, cleanup, input };
}

test("successful submission accepts an optional name and clears only an acknowledged draft", async () => {
  let payload;
  const fixture = feedbackFixture({
    value: { ...review, name: "  Ada  " },
    submitReview: async (value) => {
      payload = value;
      return { accepted: true, submission_id: value.submission_id };
    },
  });
  await fixture.form.emit("submit");
  assert.equal(payload.name, "Ada");
  assert.match(payload.submission_id, /^[\da-f-]{36}$/);
  assert.equal(payload.rating, 8);
  assert.match(
    fixture.status.textContent,
    /saved to the BlueEngine repository/,
  );
  assert.equal(fixture.fields.rating.value, "");
  assert.equal(
    fixture.local.getItem(`blueengine.feedback.v1.${game.slug}`),
    undefined,
  );
  fixture.cleanup();
  assert.equal(
    fixture.local.getItem(`blueengine.feedback.v1.${game.slug}`),
    undefined,
  );
});

test("failed submission retains the full draft and retries with the same UUID until edited", async () => {
  const payloads = [];
  const local = storage();
  const fixture = feedbackFixture({
    local,
    submitReview: async (payload) => {
      payloads.push(payload);
      throw new Error("Offline");
    },
  });
  await fixture.form.emit("submit");
  assert.match(fixture.status.textContent, /try again/);
  assert.equal(fixture.submit.disabled, false);
  const saved = loadFeedbackDraft(game, local);
  assert.equal(saved.enjoyed, review.enjoyed);
  assert.equal(saved.submission_id, payloads[0].submission_id);
  await fixture.form.emit("submit");
  assert.equal(payloads[1].submission_id, payloads[0].submission_id);
  fixture.cleanup();
  const reopened = feedbackFixture({
    local,
    value: null,
    submitReview: async (payload) => {
      payloads.push(payload);
      throw new Error("Offline");
    },
  });
  assert.equal(reopened.fields.enjoyed.value, review.enjoyed);
  await reopened.form.emit("submit");
  assert.equal(payloads[2].submission_id, payloads[0].submission_id);
  await reopened.input({ name: "New reviewer" });
  await reopened.form.emit("submit");
  assert.notEqual(payloads[3].submission_id, payloads[0].submission_id);
  assert.equal(payloads[3].name, "New reviewer");
  reopened.cleanup();
});

test("submission waits for exact acknowledgement and blocks duplicate requests in flight", async () => {
  let finish;
  let calls = 0;
  const fixture = feedbackFixture({
    submitReview: () => {
      calls++;
      return new Promise((resolve) => {
        finish = resolve;
      });
    },
  });
  const pending = fixture.form.emit("submit");
  assert.equal(fixture.submit.disabled, true);
  assert.equal(fixture.form.getAttribute("aria-busy"), "true");
  await fixture.form.emit("submit");
  assert.equal(calls, 1);
  finish({ accepted: true, submission_id: "wrong" });
  await pending;
  assert.equal(fixture.fields.enjoyed.value, review.enjoyed);
  assert.match(fixture.status.textContent, /could not be submitted/);
  assert.equal(fixture.submit.disabled, false);
  fixture.cleanup();
});

test("edits made while a review is being submitted remain as a separate draft", async () => {
  let finish;
  let payload;
  const fixture = feedbackFixture({
    submitReview: (value) => {
      payload = value;
      return new Promise((resolve) => {
        finish = resolve;
      });
    },
  });
  const pending = fixture.form.emit("submit");
  await fixture.input({ disliked: "A newer suggestion" });
  finish({ accepted: true, submission_id: payload.submission_id });
  await pending;
  assert.equal(fixture.fields.disliked.value, "A newer suggestion");
  assert.equal(
    loadFeedbackDraft(game, fixture.local).disliked,
    "A newer suggestion",
  );
  assert.equal(loadFeedbackDraft(game, fixture.local).submission_id, "");
  assert.match(fixture.status.textContent, /separate draft/);
  fixture.cleanup();
});

test("invalid fields stay local and focus the first accessible error", async () => {
  let calls = 0;
  const fixture = feedbackFixture({
    value: { ...review, rating: "", name: "x".repeat(81) },
    submitReview: async () => {
      calls++;
    },
  });
  await fixture.form.emit("submit");
  assert.equal(calls, 0);
  assert.equal(fixture.fields.name.focused, true);
  assert.equal(fixture.fields.name.getAttribute("aria-invalid"), "true");
  assert.equal(loadFeedbackDraft(game, fixture.local).name.length, 81);
  fixture.cleanup();
});
