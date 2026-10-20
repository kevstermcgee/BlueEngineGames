# Publishing a game guide

Every catalog game has a mini-wiki on its detail page: a substantive overview,
rules and objectives, exact controls, a captioned screenshot gallery, and creation
provenance, plus story and named characters when implemented. Describe the shipped game,
including its actual win/loss/retry and progression rules. Do not invent a story,
characters, mechanics, original user prompts, or historical AI decisions.

Story and character fields are optional. Omit them for a game without a narrative
or named characters; no absence essay is required. Utilities, simulations and
abstract games still need meaningful usage rules.

## Content location

New authored games should include `games/<slug>/wiki.json` and their screenshots
under `games/<slug>/assets/wiki/`. The engine publisher exports these files with
the game. Independently released games may instead place the same structure at
`site/wiki/<slug>/wiki.json` and `site/wiki/<slug>/assets/wiki/`.
The site-owned entry takes precedence when both locations exist.

Use this shape (replace all example content with verified game details):

```json
{
  "version": 1,
  "overview": "At least 120 characters explaining the game's genre, what the player does, its distinctive mechanics and the experience offered by the released game.",
  "rules": [
    "Explain the main objective and how to achieve it.",
    "Explain a concrete constraint, hazard or resource rule.",
    "Explain progression, success and failure or recovery."
  ],
  "controls": ["Exact keyboard, mouse and supported controller inputs."],
  "story": "Optional: describe the implemented setting and premise.",
  "characters": [
    {
      "name": "The actual character name or player role",
      "description": "At least 40 characters describing this character's role, identity or abilities in the game.",
      "image": "assets/wiki/character.png"
    }
  ],
  "characters_note": "Optional context about the actual characters or their appearance.",
  "gallery": [
    {"image": "assets/wiki/opening.png", "caption": "The opening gameplay challenge.", "alt": "Describe what is visible in this opening challenge."},
    {"image": "assets/wiki/later.png", "caption": "A different later challenge.", "alt": "Describe the different situation shown in this later scene."},
    {"image": "assets/wiki/action.png", "caption": "A distinctive mechanic in action.", "alt": "Describe the action and feedback visible in this capture."}
  ],
  "design": {
    "origin": "human",
    "original_prompt": "The verbatim original user request that created this game.",
    "prompt_note": "Optional context: identify excerpts or privacy redactions explicitly."
  }
}
```

For abstract or storyless games, omit `story`, `characters` and `characters_note`
when they do not apply. `story: null` and `characters: []` are also accepted.
Every listed named character still requires its description and real screenshot.

For AI-designed games, use `design.origin: "ai"`, `original_prompt: null` and a
`design.rationale` of at least 120 characters. Explain **why this specific game**
was selected: its intended appeal, interesting player decisions and meaningful
difference from prior games. This is a concise public design decision summary,
grounded in the original concept, not a private reasoning transcript or a
fabricated account of a historical decision. An optional `rationale_source`
can identify the recorded public concept used to recover an older rationale.

For human-designed games, preserve the actual original creation prompt, even if
AI wrote the code. A synopsis, reconstruction or later update request is not the
original prompt. Clearly identify verbatim excerpts and any necessary privacy
redactions; never publish credentials or unrelated private chat history.

## Screenshot requirements for every new game

- At least **three distinct, authentic gameplay screenshots**: landscape PNGs,
  at least 320 × 180 pixels, with descriptive captions and alternative text.
- An in-game screenshot showing **each listed character** (at least 96 × 96
  pixels). A suitable gallery capture may also illustrate a character. Explain
  first-person or dialogue-only appearances honestly.
- Show genuinely different situations, characters or challenges. Renaming the
  same image, resizing it or changing a few pixels is not a new gameplay capture.
- Use real game captures, not icons, concept art, synthetic screenshots or mockups.
  Visually inspect all captures and check captions against what is visible.
- Keep paths relative to the guide. Missing files, unsafe paths and byte-identical
  gallery images fail validation. Programmatic checks do not prove that images
  are accurate; review the screenshots against the running game.

Run `python3 site/wiki.py --game-dir games/<slug> --origin ai` (or `human`) to
validate an authored guide. The site test suite checks every bundled and independent
catalog entry before publishing; the site build also validates its actual release
rows. A failed build leaves the previously completed site intact.

## Earlier releases

`site/wiki/legacy-games.json` is the fixed roster of 50 games already published
on October 17, 2026. It allows sparse historical images and clearly labeled
unrecovered original prompts/rationales while those archives are completed.
It still requires substantive rules, controls, descriptions of implemented story
and named characters, and at least one authentic screenshot. Do not add future games to this roster.
New games must satisfy the full requirements, including multiple images and
original provenance. Do not mark a guide as legacy inside its own JSON.
