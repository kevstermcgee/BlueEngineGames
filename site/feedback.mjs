const ENGINE = "https://github.com/kevstermcgee/BlueEngine";
export const FEEDBACK_FORMAT = "blueengine-player-feedback-v1";
export const MAX_FEEDBACK_LENGTH = 2000;
export const MAX_REVIEWER_NAME_LENGTH = 80;
export const FEEDBACK_SUBMIT_URL = "https://blue-engine.duckdns.org/feedback";
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
const submissionIdPattern =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
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
  if (
    review.name !== undefined &&
    (typeof review.name !== "string" ||
      review.name.length > MAX_REVIEWER_NAME_LENGTH)
  )
    errors.name = "Keep your name within 80 characters.";
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
    name: "",
    submission_id: "",
    version: initialVersion(game),
    rating: "",
    enjoyed: "",
    disliked: "",
  };
  try {
    const saved = JSON.parse(storage?.getItem(draftKey(game)) || "null");
    if (!saved || saved.format !== FEEDBACK_FORMAT) return fallback;
    return {
      name: typeof saved.name === "string" ? saved.name : "",
      submission_id: submissionIdPattern.test(saved.submission_id)
        ? saved.submission_id
        : "",
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

export function buildFeedbackSubmission(game, review, submissionId) {
  const errors = validateReview(review);
  if (Object.keys(errors).length) return { errors };
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(game.slug))
    throw new Error("Invalid game identity");
  if (!submissionIdPattern.test(submissionId))
    throw new Error("Invalid submission identity");
  return {
    errors: {},
    payload: {
      format: FEEDBACK_FORMAT,
      submission_id: submissionId,
      game: game.slug,
      name: (review.name || "").trim(),
      version: review.version.trim(),
      rating: Number(review.rating),
      enjoyed: review.enjoyed.trim(),
      disliked: review.disliked.trim(),
    },
  };
}

export async function submitFeedback(
  payload,
  { fetcher = globalThis.fetch, signal, url = FEEDBACK_SUBMIT_URL } = {},
) {
  if (typeof fetcher !== "function") throw new Error("Feedback unavailable");
  const controller = new AbortController();
  const abort = () => controller.abort();
  if (signal?.aborted) abort();
  signal?.addEventListener("abort", abort, { once: true });
  const timeout = setTimeout(abort, 60000);
  try {
    const response = await fetcher(url, {
      method: "POST",
      credentials: "omit",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
      signal: controller.signal,
    });
    if (!response.ok) throw new Error("Feedback unavailable");
    const result = await response.json();
    if (
      result?.accepted !== true ||
      result.submission_id !== payload.submission_id
    )
      throw new Error("Feedback was not acknowledged");
    return result;
  } finally {
    clearTimeout(timeout);
    signal?.removeEventListener("abort", abort);
  }
}

function createSubmissionId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  const bytes = Uint8Array.from({ length: 16 }, () =>
    Math.floor(Math.random() * 256),
  );
  globalThis.crypto?.getRandomValues?.(bytes);
  bytes[6] = (bytes[6] & 15) | 64;
  bytes[8] = (bytes[8] & 63) | 128;
  const hex = Array.from(bytes)
    .map((value) => value.toString(16).padStart(2, "0"))
    .join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function renderFeedback(game) {
  return `<section class="feedback-center" id="guide-feedback" aria-labelledby="feedback-heading">
    <div class="feedback-header"><div><p class="feedback-kicker">HELP SHAPE WHAT COMES NEXT</p><h2 id="feedback-heading">Feedback center</h2></div></div>
    <p>How was ${esc(game.name)}? Share what worked, what didn’t, and what you would like to see next. The engine uses these reviews to guide future improvements.</p>
    <h3 class="feedback-public-heading">Player scores &amp; reviews</h3><p class="feedback-note">Read published reviews here without signing in. Scores include all published reviews, across game versions.</p><div class="feedback-reviews" data-feedback-reviews aria-live="polite" tabindex="-1"><p class="feedback-note">Loading player reviews…</p></div>
    <form data-feedback-form novalidate>
      <div class="feedback-name"><label for="feedback-name">Your name <span>(optional)</span></label><input id="feedback-name" name="name" maxlength="80" autocomplete="nickname" placeholder="Anonymous" aria-describedby="feedback-name-error"><p class="feedback-error" id="feedback-name-error" data-feedback-error="name"></p></div>
      <div class="feedback-fields"><div><label for="feedback-rating">Your rating <span>(out of 10)</span></label><select id="feedback-rating" name="rating" required aria-describedby="feedback-rating-error"><option value="">Choose a rating</option>${Array.from({ length: 10 }, (_, i) => `<option value="${i + 1}">${i + 1} / 10</option>`).join("")}</select><p class="feedback-error" id="feedback-rating-error" data-feedback-error="rating"></p></div>
      <div><label for="feedback-version">Version played <span>(optional)</span></label><input id="feedback-version" name="version" maxlength="160" value="${esc(initialVersion(game))}" aria-describedby="feedback-version-error"><p class="feedback-error" id="feedback-version-error" data-feedback-error="version"></p></div></div>
      <label for="feedback-enjoyed">What did you enjoy?</label><textarea id="feedback-enjoyed" name="enjoyed" rows="4" maxlength="2000" placeholder="A favorite mechanic, a memorable moment, a character you liked…" aria-describedby="feedback-enjoyed-count feedback-enjoyed-error"></textarea><div class="feedback-field-meta"><p class="feedback-error" id="feedback-enjoyed-error" data-feedback-error="enjoyed"></p><span id="feedback-enjoyed-count" data-feedback-count="enjoyed">0 / 2,000</span></div>
      <label for="feedback-disliked">What could be better?</label><textarea id="feedback-disliked" name="disliked" rows="4" maxlength="2000" placeholder="Something confusing, a frustrating rule, a change you would enjoy…" aria-describedby="feedback-disliked-count feedback-disliked-error"></textarea><div class="feedback-field-meta"><p class="feedback-error" id="feedback-disliked-error" data-feedback-error="disliked"></p><span id="feedback-disliked-count" data-feedback-count="disliked">0 / 2,000</span></div>
      <p class="feedback-note">Add at least one written answer. No account is needed. Your name is optional; leave it blank to appear as Anonymous. Feedback is public and saved in the BlueEngine repository to guide improvement cycles. Don’t include private information.</p>
      <div class="feedback-actions"><button type="submit">Submit feedback</button><span data-feedback-draft class="feedback-note">Your draft stays on this device.</span></div>
      <p class="feedback-status" data-feedback-status role="status"></p>
    </form>
  </section>`;
}

function reviewIdentity(record) {
  return submissionIdPattern.test(record.submission_id)
    ? `submission:${record.submission_id.toLowerCase()}`
    : `issue:${record.issue_number}`;
}

function validReview(record, slug) {
  if (
    !record ||
    record.game !== slug ||
    !Number.isInteger(record.rating) ||
    record.rating < 1 ||
    record.rating > 10 ||
    typeof record.version !== "string" ||
    Array.from(record.version).length > 160 ||
    typeof record.enjoyed !== "string" ||
    typeof record.disliked !== "string" ||
    Array.from(record.enjoyed).length > MAX_FEEDBACK_LENGTH ||
    Array.from(record.disliked).length > MAX_FEEDBACK_LENGTH
  )
    return false;
  if (
    record.name !== undefined &&
    (typeof record.name !== "string" ||
      Array.from(record.name).length > MAX_REVIEWER_NAME_LENGTH)
  )
    return false;
  if (record.submission_id !== undefined)
    return submissionIdPattern.test(record.submission_id);
  return (
    typeof record.author === "string" &&
    Number.isInteger(record.issue_number) &&
    record.issue_number > 0 &&
    record.source_url === `${ENGINE}/issues/${record.issue_number}`
  );
}

function validateSummaryDocument(document) {
  if (
    ![1, 2].includes(document?.schema_version) ||
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
  return `<div class="feedback-review-list">${reviews.map((review) => `<article class="feedback-review"><div><h3>${esc((review.name ?? review.author ?? "").trim() || "Anonymous")}</h3><strong>${review.rating} / 10</strong></div><p class="feedback-note">Version ${esc(review.version || "not specified")}</p>${review.enjoyed && review.enjoyed !== "_No response_" ? `<h4>Enjoyed</h4><p>${esc(review.enjoyed)}</p>` : ""}${review.disliked && review.disliked !== "_No response_" ? `<h4>Could be better</h4><p>${esc(review.disliked)}</p>` : ""}</article>`).join("")}</div>`;
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
  if (![1, 2].includes(document?.schema_version) || document.game !== game.slug)
    throw new Error("Invalid review collection");
  validateScore(document);
  if (
    document.reviews.length !== document.count ||
    document.reviews.some((review) => !validReview(review, game.slug)) ||
    new Set(document.reviews.map(reviewIdentity)).size !== document.count
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
  { storage, fetcher = globalThis.fetch, submitReview } = {},
) {
  const form = root.querySelector("[data-feedback-form]");
  if (!form) throw new Error("Feedback markup is missing");
  try {
    storage ??= globalThis.localStorage;
  } catch {
    /* unavailable in private or restricted contexts */
  }
  submitReview ||= (payload, options) =>
    submitFeedback(payload, { ...options, fetcher });
  const fields = Object.fromEntries(
    ["name", "version", "rating", "enjoyed", "disliked"].map((name) => [
      name,
      form.elements.namedItem(name),
    ]),
  );
  const status = form.querySelector("[data-feedback-status]");
  const draftNote = form.querySelector("[data-feedback-draft]");
  const submitButton = form.querySelector('[type="submit"]');
  const controller = new AbortController();
  let alive = true;
  let submitting = false;
  let completed = false;
  const draft = loadFeedbackDraft(game, storage);
  let submissionId = draft.submission_id;
  for (const [name, input] of Object.entries(fields)) input.value = draft[name];
  const read = () =>
    Object.fromEntries(
      Object.entries(fields).map(([name, input]) => [name, input.value]),
    );
  let snapshot = JSON.stringify(read());
  const count = () => {
    for (const name of ["enjoyed", "disliked"])
      form.querySelector(`[data-feedback-count="${name}"]`).textContent =
        `${fields[name].value.length.toLocaleString()} / 2,000`;
  };
  count();
  const persist = () => {
    if (completed) return;
    const current = JSON.stringify(read());
    if (current !== snapshot) {
      submissionId = "";
      snapshot = current;
    }
    const saved = saveFeedbackDraft(
      game,
      { ...read(), submission_id: submissionId },
      storage,
    );
    draftNote.textContent = saved
      ? "Draft saved on this device."
      : "Local saving is unavailable. Keep this page open to preserve your review.";
  };
  const clearErrors = () => {
    for (const [name, input] of Object.entries(fields)) {
      input.removeAttribute("aria-invalid");
      form.querySelector(`[data-feedback-error="${name}"]`).textContent = "";
    }
  };
  const onInput = () => {
    completed = false;
    count();
    persist();
    clearErrors();
    if (!submitting) status.textContent = "";
  };
  const onSubmit = async (event) => {
    event.preventDefault();
    if (submitting) return;
    clearErrors();
    completed = false;
    persist();
    const errors = validateReview(read());
    if (Object.keys(errors).length) {
      for (const [name, message] of Object.entries(errors)) {
        fields[name].setAttribute("aria-invalid", "true");
        form.querySelector(`[data-feedback-error="${name}"]`).textContent =
          message;
      }
      fields[Object.keys(errors)[0]].focus();
      status.textContent = "Check the highlighted fields before submitting.";
      return;
    }
    submissionId ||= createSubmissionId();
    const { payload } = buildFeedbackSubmission(game, read(), submissionId);
    const submittedSnapshot = snapshot;
    persist();
    submitting = true;
    submitButton.disabled = true;
    submitButton.textContent = "Submitting…";
    form.setAttribute("aria-busy", "true");
    status.textContent = "Submitting your feedback…";
    try {
      const result = await submitReview(payload, { signal: controller.signal });
      if (
        result?.accepted !== true ||
        result.submission_id !== payload.submission_id
      )
        throw new Error("Feedback was not acknowledged");
      if (!alive) return;
      if (JSON.stringify(read()) === submittedSnapshot) {
        const empty = loadFeedbackDraft(game);
        for (const [name, input] of Object.entries(fields))
          input.value = empty[name];
        snapshot = JSON.stringify(read());
        submissionId = "";
        completed = true;
        try {
          storage?.removeItem(draftKey(game));
        } catch {
          // An empty replacement prevents an accepted review being retried on this device.
          saveFeedbackDraft(game, { ...read(), submission_id: "" }, storage);
        }
        count();
        draftNote.textContent = "Your feedback has been submitted.";
        status.textContent =
          "Thank you! Your feedback was saved to the BlueEngine repository for future improvement cycles. Published reviews update after processing.";
      } else {
        persist();
        status.textContent =
          "Thank you! Your feedback was saved to the BlueEngine repository. Your newer changes remain saved as a separate draft.";
      }
    } catch {
      if (alive) {
        persist();
        status.textContent =
          "Feedback could not be submitted right now. Your draft is still here. Check your connection and try again; retrying won’t create a duplicate review.";
      }
    } finally {
      submitting = false;
      if (alive) {
        submitButton.disabled = false;
        submitButton.textContent = "Submit feedback";
        form.removeAttribute("aria-busy");
      }
    }
  };
  form.addEventListener("input", onInput);
  form.addEventListener("submit", onSubmit);
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
          "All reviews are unavailable right now. The recent reviews above remain available. Please retry in a moment.";
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
            '<p class="feedback-note">Player reviews are unavailable right now. You can still write and submit your feedback.</p>';
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
    root.removeEventListener("click", onReviewBrowse);
  };
}
