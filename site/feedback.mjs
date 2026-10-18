const ENGINE = "https://github.com/kevstermcgee/BlueEngine";
export const FEEDBACK_FORMAT = "blueengine-player-feedback-v1";
export const MAX_FEEDBACK_LENGTH = 2000;
export const MAX_HANDOFF_LENGTH = 7500;
export const SUMMARY_URL =
  "https://raw.githubusercontent.com/kevstermcgee/BlueEngine/main/docs/player-feedback/summary.json";
const summaryRequests = new WeakMap();
export const REVIEW_PAGE_SIZE = 6;

export function reviewDataUrl(game) {
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(game.slug))
    throw new Error("Invalid game identity");
  return `${SUMMARY_URL.slice(0, -"summary.json".length)}games/${game.slug}.json`;
}

async function fetchDocument(url, fetcher, signal) {
  if (typeof fetcher !== "function") throw new Error("Reviews unavailable");
  const controller = new AbortController();
  const abort = () => controller.abort();
  if (signal?.aborted) abort();
  signal?.addEventListener("abort", abort, { once: true });
  const timeout = setTimeout(abort, 10000);
  try {
    const response = await fetcher(url, {
      signal: controller.signal,
      cache: "no-cache",
      credentials: "omit",
    });
    if (!response.ok) throw new Error("Reviews unavailable");
    return await response.json();
  } finally {
    clearTimeout(timeout);
    signal?.removeEventListener("abort", abort);
  }
}

// Cards and an open wiki share one short-lived public request, including in the launcher.
export function loadPublicReviewSummary({
  fetcher = globalThis.fetch,
  force = false,
} = {}) {
  if (typeof fetcher !== "function")
    return Promise.reject(new Error("Reviews unavailable"));
  const cached = summaryRequests.get(fetcher);
  if (
    cached &&
    (!force || cached.pending) &&
    (cached.pending || cached.expires > Date.now())
  )
    return cached.promise;
  const entry = { pending: true, expires: Date.now() + 5 * 60 * 1000 };
  entry.promise = fetchDocument(SUMMARY_URL, fetcher)
    .then((document) => {
      validateSummaryDocument(document);
      return document;
    })
    .catch((error) => {
      if (summaryRequests.get(fetcher) === entry)
        summaryRequests.delete(fetcher);
      throw error;
    })
    .finally(() => {
      entry.pending = false;
    });
  summaryRequests.set(fetcher, entry);
  return entry.promise;
}
const esc = (value = "") =>
  String(value).replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ],
  );
const draftKey = (game) => `blueengine.feedback.v1.${game.slug}`;
const initialVersion = (game) =>
  String(
    game.installedVersion ||
      game.version ||
      game.package?.release ||
      "Unknown version",
  ).slice(0, 160);

export function validateReview(review) {
  const errors = {};
  if (!/^(?:[1-9]|10)$/.test(String(review.rating)))
    errors.rating = "Choose a whole-number rating from 1 to 10.";
  if (typeof review.version !== "string" || review.version.length > 160)
    errors.version = "Keep the version within 160 characters.";
  for (const field of ["enjoyed", "disliked"]) {
    if (
      typeof review[field] !== "string" ||
      review[field].length > MAX_FEEDBACK_LENGTH
    )
      errors[field] = "Keep this answer within 2,000 characters.";
  }
  if (
    !String(review.enjoyed || "").trim() &&
    !String(review.disliked || "").trim()
  )
    errors.enjoyed =
      "Tell us something you enjoyed or something that could be better.";
  return errors;
}

export function loadFeedbackDraft(game, storage) {
  const fallback = {
    version: initialVersion(game),
    rating: "",
    enjoyed: "",
    disliked: "",
  };
  try {
    const saved = JSON.parse(storage?.getItem(draftKey(game)) || "null");
    if (!saved || saved.format !== FEEDBACK_FORMAT) return fallback;
    return {
      version:
        typeof saved.version === "string"
          ? saved.version.slice(0, 160)
          : fallback.version,
      rating: /^(?:[1-9]|10)$/.test(String(saved.rating))
        ? String(saved.rating)
        : "",
      // Keep every character of a saved review; validation prevents oversized submissions.
      enjoyed: typeof saved.enjoyed === "string" ? saved.enjoyed : "",
      disliked: typeof saved.disliked === "string" ? saved.disliked : "",
    };
  } catch {
    return fallback;
  }
}

export function saveFeedbackDraft(game, review, storage) {
  try {
    if (!storage) return false;
    storage.setItem(
      draftKey(game),
      JSON.stringify({ ...review, format: FEEDBACK_FORMAT }),
    );
    return true;
  } catch {
    return false;
  }
}

function fenced(value) {
  const longest = Math.max(
    2,
    ...(String(value).match(/`+/g) || []).map((s) => s.length),
  );
  const fence = "`".repeat(longest + 1);
  return `${fence}text\n${value}\n${fence}`;
}

export function formatReviewMarkdown(game, review) {
  return [
    ["Game", game.slug],
    ["Game version", review.version.trim() || "_No response_"],
    ["Rating (1–10)", String(review.rating)],
    ["What I enjoyed", review.enjoyed.trim() || "_No response_"],
    ["What could be better", review.disliked.trim() || "_No response_"],
    ["Feedback format", FEEDBACK_FORMAT],
  ]
    .map(([heading, value]) => `### ${heading}\n\n${fenced(value)}`)
    .join("\n\n");
}

export function buildFeedbackHandoff(game, review) {
  const errors = validateReview(review);
  if (Object.keys(errors).length) return { errors };
  const url = new URL(`${ENGINE}/issues/new`);
  url.searchParams.set("template", "player-feedback.yml");
  const title = `[Player feedback] ${game.name} — ${review.rating}/10`;
  url.searchParams.set("title", title);
  for (const [field, value] of Object.entries({
    game: game.slug,
    version: review.version.trim(),
    rating: String(review.rating),
    enjoyed: review.enjoyed.trim(),
    disliked: review.disliked.trim(),
    format: FEEDBACK_FORMAT,
  }))
    url.searchParams.set(field, value);
  const fullUrl = url.href;
  const blankUrl = new URL(`${ENGINE}/issues/new`);
  blankUrl.searchParams.set("title", title);
  // Browsers and GitHub can reject long query strings, particularly with Unicode.
  // Preserve the complete body for an explicit copy-and-paste handoff instead.
  return {
    errors: {},
    title,
    url: fullUrl.length <= MAX_HANDOFF_LENGTH ? fullUrl : blankUrl.href,
    copyUrl: blankUrl.href,
    needsCopy: fullUrl.length > MAX_HANDOFF_LENGTH,
    markdown: formatReviewMarkdown(game, review),
  };
}

export function reviewListUrl(game) {
  const url = new URL(`${ENGINE}/issues`);
  url.searchParams.set(
    "q",
    `is:issue "${FEEDBACK_FORMAT}" "${game.slug}" in:body`,
  );
  return url.href;
}

export function renderFeedback(game) {
  return `<section class="feedback-center" id="guide-feedback" aria-labelledby="feedback-heading">
    <div class="feedback-header"><div><p class="feedback-kicker">HELP SHAPE WHAT COMES NEXT</p><h2 id="feedback-heading">Feedback center</h2></div><a data-feedback-external href="${esc(reviewListUrl(game))}" target="_blank" rel="noopener noreferrer">Reviews on GitHub ↗</a></div>
    <p>How was ${esc(game.name)}? Share what worked, what didn’t, and what you would like to see next. The engine uses these reviews to guide future improvements.</p>
    <h3 class="feedback-public-heading">Player scores &amp; reviews</h3><p class="feedback-note">Read published reviews here without signing in. Scores include all published reviews, across game versions.</p><div class="feedback-reviews" data-feedback-reviews aria-live="polite" tabindex="-1"><p class="feedback-note">Loading player reviews…</p></div>
    <form data-feedback-form novalidate>
      <div class="feedback-fields"><div><label for="feedback-rating">Your rating <span>(out of 10)</span></label><select id="feedback-rating" name="rating" required aria-describedby="feedback-rating-error"><option value="">Choose a rating</option>${Array.from({ length: 10 }, (_, i) => `<option value="${i + 1}">${i + 1} / 10</option>`).join("")}</select><p class="feedback-error" id="feedback-rating-error" data-feedback-error="rating"></p></div>
      <div><label for="feedback-version">Version played <span>(optional)</span></label><input id="feedback-version" name="version" maxlength="160" value="${esc(initialVersion(game))}" aria-describedby="feedback-version-error"><p class="feedback-error" id="feedback-version-error" data-feedback-error="version"></p></div></div>
      <label for="feedback-enjoyed">What did you enjoy?</label><textarea id="feedback-enjoyed" name="enjoyed" rows="4" maxlength="2000" placeholder="A favorite mechanic, a memorable moment, a character you liked…" aria-describedby="feedback-enjoyed-count feedback-enjoyed-error"></textarea><div class="feedback-field-meta"><p class="feedback-error" id="feedback-enjoyed-error" data-feedback-error="enjoyed"></p><span id="feedback-enjoyed-count" data-feedback-count="enjoyed">0 / 2,000</span></div>
      <label for="feedback-disliked">What could be better?</label><textarea id="feedback-disliked" name="disliked" rows="4" maxlength="2000" placeholder="Something confusing, a frustrating rule, a change you would enjoy…" aria-describedby="feedback-disliked-count feedback-disliked-error"></textarea><div class="feedback-field-meta"><p class="feedback-error" id="feedback-disliked-error" data-feedback-error="disliked"></p><span id="feedback-disliked-count" data-feedback-count="disliked">0 / 2,000</span></div>
      <p class="feedback-note">Add at least one written answer. Reviews are public in the BlueEngine repository. GitHub sign-in is required; review and publish the issue on GitHub to submit your feedback. Don’t include private information.</p>
      <div class="feedback-actions"><button type="submit">Review &amp; submit on GitHub ↗</button><span data-feedback-draft class="feedback-note">Your draft stays on this device.</span></div>
      <p data-feedback-manual hidden><a data-feedback-open-copy href="${ENGINE}/issues/new?template=player-feedback.yml" target="_blank" rel="noopener noreferrer">Open GitHub review form ↗</a></p>
      <div class="feedback-copy" data-feedback-copy hidden><p>To submit manually, copy the complete review, open a blank GitHub issue, and paste it into the issue body. The title is included in the link below.</p><label for="feedback-copy-title">Issue title</label><input id="feedback-copy-title" data-feedback-copy-title readonly><label for="feedback-copy-body">Complete review</label><textarea id="feedback-copy-body" data-feedback-copy-body rows="10" readonly></textarea><div class="feedback-actions"><button type="button" data-feedback-copy-button>Copy review</button><a data-feedback-open-copy href="${ENGINE}/issues/new" target="_blank" rel="noopener noreferrer">Open blank GitHub issue ↗</a></div></div>
      <p class="feedback-status" data-feedback-status role="status"></p>
    </form>
  </section>`;
}

function validReview(record, slug) {
  return (
    record &&
    record.game === slug &&
    Number.isInteger(record.rating) &&
    record.rating >= 1 &&
    record.rating <= 10 &&
    typeof record.author === "string" &&
    typeof record.version === "string" &&
    typeof record.enjoyed === "string" &&
    typeof record.disliked === "string" &&
    Array.from(record.enjoyed).length <= MAX_FEEDBACK_LENGTH &&
    Array.from(record.disliked).length <= MAX_FEEDBACK_LENGTH &&
    Number.isInteger(record.issue_number) &&
    record.issue_number > 0 &&
    record.source_url === `${ENGINE}/issues/${record.issue_number}`
  );
}

function validateSummaryDocument(document) {
  if (
    document?.schema_version !== 1 ||
    !document.games ||
    typeof document.games !== "object" ||
    Array.isArray(document.games)
  )
    throw new Error("Invalid review summary");
}

function validateScore(summary) {
  if (
    !summary ||
    !Number.isInteger(summary.count) ||
    summary.count < 0 ||
    (summary.count === 0
      ? summary.average !== null
      : !Number.isFinite(summary.average) ||
        summary.average < 1 ||
        summary.average > 10) ||
    !Array.isArray(summary.reviews) ||
    summary.reviews.length > summary.count
  )
    throw new Error("Invalid review summary");
}

export function readReviewSummary(document, game) {
  validateSummaryDocument(document);
  const summary = Object.hasOwn(document.games, game.slug)
    ? document.games[game.slug]
    : { count: 0, average: null, reviews: [] };
  validateScore(summary);
  if (summary.reviews.some((review) => !validReview(review, game.slug)))
    throw new Error("Invalid review entry");
  return summary;
}

export function renderPublicScore(
  game,
  { document, unavailable = false } = {},
) {
  if (unavailable)
    return '<span class="public-score-empty">Reviews unavailable</span>';
  if (!document)
    return '<span class="public-score-empty">Loading player scores…</span>';
  try {
    const summary = readReviewSummary(document, game);
    return summary.count
      ? `<strong>${summary.average.toFixed(1)}<span> / 10</span></strong><span> · ${summary.count} ${summary.count === 1 ? "review" : "reviews"}</span>`
      : '<span class="public-score-empty">No reviews yet · Be the first</span>';
  } catch {
    return '<span class="public-score-empty">Reviews unavailable</span>';
  }
}

export async function mountPublicScores(root, options = {}) {
  let document;
  let unavailable = false;
  try {
    document = await loadPublicReviewSummary(options);
  } catch {
    unavailable = true;
  }
  for (const node of root.querySelectorAll("[data-public-score]"))
    node.innerHTML = renderPublicScore(
      { slug: node.dataset.publicScore },
      { document, unavailable },
    );
  return document;
}

function renderReviews(reviews) {
  return `<div class="feedback-review-list">${reviews.map((review) => `<article class="feedback-review"><div><h3>${esc(review.author)}</h3><strong>${review.rating} / 10</strong></div><p class="feedback-note">Version ${esc(review.version || "not specified")}</p>${review.enjoyed && review.enjoyed !== "_No response_" ? `<h4>Enjoyed</h4><p>${esc(review.enjoyed)}</p>` : ""}${review.disliked && review.disliked !== "_No response_" ? `<h4>Could be better</h4><p>${esc(review.disliked)}</p>` : ""}<a data-feedback-external href="${esc(review.source_url)}" target="_blank" rel="noopener noreferrer">Review #${review.issue_number} on GitHub ↗</a></article>`).join("")}</div>`;
}

function renderScore(summary) {
  return `<p class="feedback-score"><strong>${summary.average.toFixed(1)}<span> / 10</span></strong><span>${summary.count} ${summary.count === 1 ? "player review" : "player reviews"}</span></p>`;
}

const noReviews =
  '<p class="feedback-note">No published reviews yet. Be the first to share your experience.</p>';

export function renderReviewSummary(document, game) {
  const summary = readReviewSummary(document, game);
  if (!summary.count) return noReviews;
  const reviews = summary.reviews.slice(0, 3);
  return `${renderScore(summary)}${renderReviews(reviews)}${summary.count > reviews.length ? `<div class="feedback-pagination"><p class="feedback-note">Showing the ${reviews.length} most recent reviews.</p><button type="button" data-feedback-browse>Browse all ${summary.count} reviews</button></div>` : ""}<p class="feedback-note">New feedback appears after repository processing.</p>`;
}

export function readReviewCollection(document, game) {
  if (document?.schema_version !== 1 || document.game !== game.slug)
    throw new Error("Invalid review collection");
  validateScore(document);
  if (
    document.reviews.length !== document.count ||
    document.reviews.some((review) => !validReview(review, game.slug)) ||
    new Set(document.reviews.map((review) => review.issue_number)).size !==
      document.count
  )
    throw new Error("Invalid review collection");
  return document;
}

export function renderReviewPage(document, game, { page = 0 } = {}) {
  const collection = readReviewCollection(document, game);
  if (!collection.count) return noReviews;
  const pages = Math.ceil(collection.count / REVIEW_PAGE_SIZE);
  if (!Number.isInteger(page) || page < 0 || page >= pages)
    throw new Error("Invalid review page");
  const offset = page * REVIEW_PAGE_SIZE;
  return `${renderScore(collection)}${renderReviews(collection.reviews.slice(offset, offset + REVIEW_PAGE_SIZE))}<div class="feedback-pagination"><p class="feedback-note">Showing ${offset + 1}–${Math.min(offset + REVIEW_PAGE_SIZE, collection.count)} of ${collection.count} reviews · Newest first</p><div><button type="button" data-feedback-page="${page - 1}" ${page === 0 ? "disabled" : ""}>Previous</button><button type="button" data-feedback-page="${page + 1}" ${page + 1 === pages ? "disabled" : ""}>Next</button></div></div><p class="feedback-note">New feedback appears after repository processing.</p>`;
}

export function mountFeedback(
  root,
  game,
  { openExternal, storage, fetcher = globalThis.fetch, clipboard } = {},
) {
  const form = root.querySelector("[data-feedback-form]");
  if (!form) throw new Error("Feedback markup is missing");
  try {
    storage ??= globalThis.localStorage;
  } catch {
    /* unavailable in private or restricted contexts */
  }
  try {
    clipboard ??= globalThis.navigator?.clipboard;
  } catch {
    /* copy box is always available */
  }
  openExternal ||= (url) => {
    const opened = globalThis.open(url, "_blank", "noopener,noreferrer");
    // noopener can return null for a successfully opened window; keep manual links visible.
    return opened;
  };
  const fields = Object.fromEntries(
    ["version", "rating", "enjoyed", "disliked"].map((name) => [
      name,
      form.elements.namedItem(name),
    ]),
  );
  const status = form.querySelector("[data-feedback-status]");
  const draftNote = form.querySelector("[data-feedback-draft]");
  const copyBox = form.querySelector("[data-feedback-copy]");
  const manualLink = form.querySelector("[data-feedback-manual]");
  let alive = true;
  let handoff = null;
  const draft = loadFeedbackDraft(game, storage);
  for (const [name, value] of Object.entries(draft)) fields[name].value = value;
  const read = () =>
    Object.fromEntries(
      Object.entries(fields).map(([name, input]) => [name, input.value]),
    );
  const count = () => {
    for (const name of ["enjoyed", "disliked"])
      form.querySelector(`[data-feedback-count="${name}"]`).textContent =
        `${fields[name].value.length.toLocaleString()} / 2,000`;
  };
  count();
  const persist = () => {
    const saved = saveFeedbackDraft(game, read(), storage);
    draftNote.textContent = saved
      ? "Draft saved on this device."
      : "Local saving is unavailable. Keep this page open or copy your review.";
  };
  const clearErrors = () => {
    for (const [name, input] of Object.entries(fields)) {
      input.removeAttribute("aria-invalid");
      form.querySelector(`[data-feedback-error="${name}"]`).textContent = "";
    }
  };
  const onInput = () => {
    count();
    persist();
    clearErrors();
    copyBox.hidden = true;
    manualLink.hidden = true;
    status.textContent = "";
  };
  const open = async (url, submission = false) => {
    try {
      await openExternal(url);
    } catch {
      if (alive) {
        if (submission) copyBox.hidden = false;
        status.textContent =
          "Could not open GitHub. Retry the review-form link or copy your complete review below and submit it manually.";
      }
    }
  };
  const onSubmit = async (event) => {
    event.preventDefault();
    clearErrors();
    persist();
    handoff = buildFeedbackHandoff(game, read());
    if (Object.keys(handoff.errors).length) {
      for (const [name, message] of Object.entries(handoff.errors)) {
        fields[name].setAttribute("aria-invalid", "true");
        form.querySelector(`[data-feedback-error="${name}"]`).textContent =
          message;
      }
      fields[Object.keys(handoff.errors)[0]].focus();
      status.textContent = "Check the highlighted fields before continuing.";
      return;
    }
    form.querySelector("[data-feedback-copy-title]").value = handoff.title;
    form.querySelector("[data-feedback-copy-body]").value = handoff.markdown;
    manualLink.querySelector("[data-feedback-open-copy]").href = handoff.url;
    copyBox.querySelector("[data-feedback-open-copy]").href = handoff.copyUrl;
    if (handoff.needsCopy) {
      copyBox.hidden = false;
      status.textContent =
        "Your full review is ready to copy. It has not been published yet.";
      copyBox.scrollIntoView?.({ block: "nearest" });
      return;
    }
    manualLink.hidden = false;
    status.textContent =
      "Continue on GitHub to publish your review. Your draft stays saved here until you change it.";
    await open(handoff.url, true);
  };
  const onCopy = async () => {
    if (!handoff) return;
    const body = form.querySelector("[data-feedback-copy-body]");
    try {
      if (!clipboard?.writeText) throw new Error("Clipboard unavailable");
      await clipboard.writeText(handoff.markdown);
      if (alive)
        status.textContent =
          "Review copied. Open a blank GitHub issue, paste the review, check the title above, then publish the issue on GitHub.";
    } catch {
      body.focus();
      body.select();
      status.textContent =
        "Select and copy the complete review above, then paste it into a blank GitHub issue.";
    }
  };
  const onExternal = (event) => {
    const link = event.target.closest?.(
      "[data-feedback-external], [data-feedback-open-copy]",
    );
    if (link && root.contains(link)) {
      event.preventDefault();
      void open(link.href);
    }
  };
  form.addEventListener("input", onInput);
  form.addEventListener("submit", onSubmit);
  form
    .querySelector("[data-feedback-copy-button]")
    .addEventListener("click", onCopy);
  root.addEventListener("click", onExternal);
  const controller = new AbortController();
  const reviews = root.querySelector("[data-feedback-reviews]");
  let collection = null;
  let loadingCollection = false;
  const onReviewBrowse = async (event) => {
    const button = event.target.closest?.(
      "[data-feedback-browse], [data-feedback-page]",
    );
    if (!button || !reviews.contains(button) || loadingCollection) return;
    if (collection && button.hasAttribute("data-feedback-page")) {
      reviews.innerHTML = renderReviewPage(collection, game, {
        page: Number(button.dataset.feedbackPage),
      });
      reviews.focus({ preventScroll: true });
      return;
    }
    loadingCollection = true;
    button.disabled = true;
    button.textContent = "Loading all reviews…";
    reviews.querySelector("[data-feedback-load-error]")?.remove();
    try {
      collection = readReviewCollection(
        await fetchDocument(reviewDataUrl(game), fetcher, controller.signal),
        game,
      );
      if (alive) {
        reviews.innerHTML = renderReviewPage(collection, game);
        reviews.focus({ preventScroll: true });
      }
    } catch {
      if (alive) {
        button.disabled = false;
        button.textContent = "Retry loading all reviews";
        const note = root.ownerDocument.createElement("p");
        note.className = "feedback-note";
        note.dataset.feedbackLoadError = "";
        note.setAttribute("role", "status");
        note.textContent =
          "All reviews are unavailable right now. The recent reviews above remain available; retry or read reviews on GitHub.";
        reviews.append(note);
      }
    } finally {
      loadingCollection = false;
    }
  };
  root.addEventListener("click", onReviewBrowse);
  if (typeof fetcher === "function") {
    loadPublicReviewSummary({ fetcher })
      .then((document) => {
        if (alive) reviews.innerHTML = renderReviewSummary(document, game);
      })
      .catch(() => {
        if (alive)
          reviews.innerHTML =
            '<p class="feedback-note">Player reviews are unavailable right now. You can still write a review or read published reviews on GitHub.</p>';
      });
  } else
    reviews.innerHTML =
      '<p class="feedback-note">Connect to view published reviews. You can still write a review offline.</p>';
  return () => {
    persist();
    alive = false;
    controller.abort();
    form.removeEventListener("input", onInput);
    form.removeEventListener("submit", onSubmit);
    form
      .querySelector("[data-feedback-copy-button]")
      .removeEventListener("click", onCopy);
    root.removeEventListener("click", onExternal);
    root.removeEventListener("click", onReviewBrowse);
  };
}
