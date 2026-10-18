const ENGINE = "https://github.com/kevstermcgee/BlueEngine";
export const FEEDBACK_FORMAT = "blueengine-player-feedback-v1";
export const MAX_FEEDBACK_LENGTH = 2000;
export const MAX_HANDOFF_LENGTH = 7500;
const SUMMARY_URL =
  "https://raw.githubusercontent.com/kevstermcgee/BlueEngine/main/docs/player-feedback/summary.json";
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
    <div class="feedback-header"><div><p class="feedback-kicker">HELP SHAPE WHAT COMES NEXT</p><h2 id="feedback-heading">Feedback center</h2></div><a data-feedback-external href="${esc(reviewListUrl(game))}" target="_blank" rel="noopener noreferrer">All player reviews ↗</a></div>
    <p>How was ${esc(game.name)}? Share what worked, what didn’t, and what you would like to see next. The engine uses these reviews to guide future improvements.</p>
    <div class="feedback-reviews" data-feedback-reviews aria-live="polite"><p class="feedback-note">Loading player reviews…</p></div>
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
    record.enjoyed.length <= MAX_FEEDBACK_LENGTH &&
    record.disliked.length <= MAX_FEEDBACK_LENGTH &&
    Number.isInteger(record.issue_number) &&
    record.source_url === `${ENGINE}/issues/${record.issue_number}`
  );
}

export function renderReviewSummary(document, game) {
  if (
    document?.schema_version !== 1 ||
    !document.games ||
    typeof document.games !== "object"
  )
    throw new Error("Invalid review summary");
  const summary = Object.hasOwn(document.games, game.slug)
    ? document.games[game.slug]
    : null;
  if (!summary || summary.count === 0)
    return '<p class="feedback-note">No published reviews yet. Be the first to share your experience.</p>';
  if (
    !Number.isInteger(summary.count) ||
    summary.count < 1 ||
    !Number.isFinite(summary.average) ||
    summary.average < 1 ||
    summary.average > 10 ||
    !Array.isArray(summary.reviews)
  )
    throw new Error("Invalid review summary");
  const reviews = summary.reviews.slice(0, 3);
  if (reviews.some((review) => !validReview(review, game.slug)))
    throw new Error("Invalid review entry");
  return `<p class="feedback-score"><strong>${esc(summary.average.toFixed(1))}<span> / 10</span></strong><span>${summary.count} ${summary.count === 1 ? "player review" : "player reviews"}</span></p><div class="feedback-review-list">${reviews.map((review) => `<article class="feedback-review"><div><h3>${esc(review.author)}</h3><strong>${review.rating} / 10</strong></div><p class="feedback-note">Version ${esc(review.version)}</p>${review.enjoyed && review.enjoyed !== "_No response_" ? `<h4>Enjoyed</h4><p>${esc(review.enjoyed)}</p>` : ""}${review.disliked && review.disliked !== "_No response_" ? `<h4>Could be better</h4><p>${esc(review.disliked)}</p>` : ""}<a data-feedback-external href="${esc(review.source_url)}" target="_blank" rel="noopener noreferrer">Read on GitHub ↗</a></article>`).join("")}</div><p class="feedback-note">Recently published reviews. New feedback appears after repository processing.</p>`;
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
  if (typeof fetcher === "function") {
    Promise.resolve()
      .then(() =>
        fetcher(SUMMARY_URL, {
          signal: controller.signal,
          cache: "no-cache",
          credentials: "omit",
        }),
      )
      .then((response) => {
        if (!response.ok) throw new Error("Reviews unavailable");
        return response.json();
      })
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
  };
}
