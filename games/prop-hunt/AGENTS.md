# Prop Hunt - AI Agent Guide

A game with its own simulation on BlueEngine (path dependency `vesper3d`, see Cargo.toml). Read this
file and `src/lib.rs`. Missing engine capability is engine work, not permission to invent an API: in the
engine checkout run `python tools/be2.py context "<need>"`, and read docs/CUSTOM_CLIENT.md (the kit and
the loop), docs/SHARED_GAMEPLAY.md ("Custom loops") and docs/NETPLAY.md (the lobby/server/client kit).

## Architecture (do not break)
- `src/sim.rs` is the round: pure, seeded (`devkit::Rng`), fixed 60 Hz `Sim::step(&Inputs)`. No window,
  sound device, wall clock or networking. It reaches the window (and, over the network, every client)
  only through `Event`s (`drain_events`). Phase FSM: `Hiding(ticks left)` -> `Seeking(ticks left)` ->
  `RoundOver`, driven entirely by tick countdowns so win conditions fire at an exact, testable tick.
- `src/layout.rs` is Hollow Pine's house: walls (`devkit::{wall_along_x, wall_along_z}`), the 14 legal
  `DISGUISE_SPOTS`, decoy prop placements (`decoys()`) and spawns — the one source both `Sim` (confirm
  radius, collision) and `main.rs`/`models.rs` (matching visuals) read, so they can never drift apart.
- `src/disguise.rs` is a curated `DisguiseKind` subset of `vesper3d::viewer::props::CATALOG`; its
  `half_extents` come straight from the engine's own catalog, so a disguised hider and a decoy of the
  same kind are sized identically by construction, not by coincidence.
- `src/netgame.rs` is the one truly novel part of this game: `PropHuntGame: NetGame` and
  `PropHuntView: ClientView`. Read the module doc at the top of the file before touching `snapshot()` —
  it is a per-participant **redaction table**, not an identical broadcast dressed up as one, and it is the
  entire reason this game needs a custom-sim `NetGame` rather than the stock `GameDocument` road (which has
  no team/role/spectator concept at all). The critical invariant, proved in `tests/wire.rs` and
  `tests/net.rs`: an untagged hider's own snapshot must never carry another still-active hider's real
  position or disguise. `Event`s are broadcast identically to every client by the engine's own netplay
  server (`netplay/server.rs`), so an `Event` must never carry that information either — only `snapshot()`
  may differ per viewer. See the "Networking" section below for the server/bots/tests that exercise this.
- `src/bot.rs` is simple AI (walk to a chosen spot and confirm a disguise; chase the nearest hider the
  bot's own snapshot shows it), reading only the same data a real client would see. Used by
  `tests/net.rs`, `tests/udp.rs` and `prop-hunt-bots`; there is no bot AI beyond this (no empty-seat
  filling, no takeover when a player leaves — see `NetGame::release` in `netgame.rs`).
- `src/main.rs` is the window: `ClientInput` + `GameShell` for devices, `devkit::Lifecycle` for the run
  flags, one `Input` per fixed tick (routed either to a `NetClient` or to a local solo-preview `Sim`),
  quick save/load (solo preview only — there is no server-side save API), `kit` renderer/HUD/sound.
  `Screen::{Select, Lobby, Hiding, Seeking, Results}` tracks the top-level flow; which one is live is
  recomputed every frame from whichever `Phase` the live snapshot (or local `Sim`) reports, not from
  hand-written transition edges, so it can never drift from what the simulation actually says.
- **No bot AI fills an empty hider seat.** `Sim` always has exactly `MAX_HIDERS` hider slots regardless of
  how many humans joined; an unseated slot simply never receives real input (`Input::default()` every
  tick), gets auto-placed with a random disguise like anyone else at the end of the hide phase, and sits
  there as a legitimate (if silent) hider the seeker can still find. This is by design, not a gap: no
  per-slot special-casing was needed anywhere in `sim.rs` to make this work.
- **A confirmed hider is frozen**: position, yaw and disguise all lock at `confirm_placement`, for the
  rest of the round. This is the seek phase's one MVP simplification (see the plan this game was built
  from): the engine's `Controller` could support a hider who keeps moving while hidden, but this game
  chose not to for v1. A hider can still look around freely after freezing — pitch is pure client-side
  state (see the next point) and is never frozen.
- **Pitch never goes on the wire.** `Controller::update`'s movement only ever depends on yaw, never pitch,
  so the server does not need the client's pitch for anything and it is not part of `PropHuntSnapshot`.
  `PropHuntView` tracks it purely client-side (`PropHuntView::pitch`) and never reconciles or corrects it
  — there is nothing for the server to disagree with the client about. Do not add pitch to the wire
  format without a real reason; it would just be bytes nobody reads.
- **Audio has no music.** `main.rs`'s `HAS_MUSIC` is `false`: the core tension mechanic is silence
  punctuated by footsteps, which a generated ambient bed would mask. This is a deliberate, already-made
  decision (not the template default) — do not turn it back on without a real design reason, and if you
  do, wire up `synth::ambient_spec_for` and the `Settings`/music-toggle plumbing properly rather than just
  flipping the flag.
- Cannot see or hear the game? Use `--capture`, `--script`, `--perf` below, and numbers. `house_look()` and
  `models.rs`'s material colours deliberately favour visibility (a bright, ordinary house, not horror
  lighting) — the hiding in this game is about disguise and position, never about the player being unable
  to see; re-darkening the `Look` without a real headless capture to check it is how the first draft of
  this game ended up rendering solid black.

## Networking
This is the first real `NetGame` in the BlueEngine family to make `snapshot(m, participant)` differ per
viewer for an actual reason (every other game to date broadcasts the same state to everyone). Read
`src/netgame.rs`'s module doc before changing anything here.
- `src/netgame.rs`: wire layouts (`write_input`/`read_input`, `write_snapshot`/`read_snapshot`,
  `write_event`/`read_event`), `start()` (one seeded-random seeker among whoever preferred it, everyone
  else a hider in seat order), and the redaction table in `snapshot()`.
- `src/bin/prop-hunt-server.rs`: the dedicated server (`NetServer<PropHuntGame, _>`). See `docs/HOSTING.md`
  for how to build, run and deploy it (the same shape as Spooky Kart's server, on a different port —
  4101, not Spooky Kart's 4100 — so both can run on the same box).
- `src/bin/prop-hunt-bots.rs`: bot clients against a real server, for load testing.
- `tests/wire.rs`: every shape round-trips, **and** the redaction table actually redacts (asserted against
  real `Sim` state, not just checked for a round-trip) — the datagram-budget and fuzz tests live here too.
- `tests/net.rs`: a full lobby -> hide -> seek -> results round over an in-memory lossy network, bot-driven
  on both sides, including `an_untagged_hiders_position_never_reaches_another_untagged_hiders_client` (the
  single most important correctness property unique to this game, checked tick-by-tick against the
  server's own ground truth) and `release()`'s two rules (a departed hider is tagged, not replaced; a
  departed seeker ends the round as a hiders' win, not handed to an AI).
- `tests/udp.rs`: the same server and clients for real, over UDP loopback, for a few seconds (a whole
  round is well over three minutes, so this does not try to finish one).

## Look, scale, iteration
- Mouse look has one convention (`devkit::look`): hand right turns right, hand up looks up. Feed
  `MouseLook::look(px_right, px_down)` from `game_input::mouse_pixels()` into `FpsCamera` (or the same
  `[right, down]` delta into `Controller::look(dx, dy, 1.0, false)`). Never read `mouse_delta_position()`
  yourself (previous-minus-current: both signs flipped); never hand-write a yaw/pitch formula.
- Gamepad: `input.look_delta_with(&shell, dt)` already adds mouse and right stick in that convention;
  menus use `input.menu_step()` / `menu_select()` / `menu_back()` (flick + repeat built in). Quit with
  `game_client::request_exit()` and break on `exit_requested()`; never `process::exit`.
- Unit = metre. Check generated geometry without a window: `Bounds::of(points)?.expect_longest("shell",
  0.05..=0.30)` fails with the scale to apply; `kit::gizmo::human_scale`/`bbox` show it in a frame.
- Rebuild with `scripts/blue dev` (closes a still-open copy first: a running .exe cannot be relinked on
  Windows) and use `scripts/blue dev -- --capture out --frames 30`; a release build is for `ship`.

## Save states
- F5 / F9 save and load the `quick` slot in the **solo preview only** (`devkit::snapshot`, wired by
  `devkit::Lifecycle` in `main.rs`); there is no server-side save API for a live networked round yet.
  `impl Snapshot for Sim` in `src/sim.rs` lists everything that decides the round's future. **Every new
  field of `Sim` or `Hider` goes into `SimState`/`HiderState`**, or a save loses it;
  `a_save_from_any_tick_resumes_as_the_game_promises...` fails at the first tick that reads what was
  forgotten, and the error names the `hash_parts` piece.
- `Sim::POLICY` is `SavePolicy::Exact` — no rigid bodies are involved (`Controller` only), so this is the
  strongest and simplest save contract; never weaken it to make a test pass for a non-physics field.
- Loads are all-or-nothing and saves are atomic with a backup; never write your own save file code.

## Checks
- `cargo test` (rules, wire, netplay and determinism, all headless). Add a test for every rule you add,
  and — if you touch the redaction table — a test that asserts it against real `Sim` state, not just a
  round-trip (see `tests/wire.rs`'s own comment on this).
- `cargo fmt` and `cargo clippy --all-targets` before calling anything done.
- Look at it (requires a display, or `Xvfb`/`xvfb-run` headless):
  `target/debug/prop-hunt --capture out --frames 30,120 --seed 3 --script "fwd:0-200,right:0-200,confirm@210"`,
  then `python <engine>/tools/contact_sheet.py sheet.png --dir out` (one image instead of many). An
  unattended run (any `--capture`/`--script`/`--exit-after`) skips the title screen straight into a local
  solo preview (hider seat 0) when no `--connect`/`server.txt` is configured, since nothing can press
  Enter for it.

## Definition of done (every game made with BlueEngine)
1. `python scripts/check.py` passes (needs `BE2_TOOLS` pointed at a built `be2-tools`; not available in
   every environment — `cargo test && cargo fmt --check && cargo clippy --all-targets` is the fallback).
   It ends with the ship gate, so it fails until step 2 is done.
2. The game ships as a package with its own icon and desktop shortcut: fill in `assets/identity.json`
   (real title, tagline, controls — already done), regenerate the icon if the title changes
   (`be2-tools icon TITLE assets --replace`), then run `scripts/blue ship`. It builds a release package
   in `dist/`, creates the shortcut named after the game, and verifies target, icon uniqueness among the
   desktop's shortcuts, window title and icon. Never point a shortcut at `target/`. The catalog copy has its own title-seeded icon set, generated with `be2-tools`.
3. You looked at real frames of the shipped exe and exercised the controls; say what you did not verify.
   As of this writing: the hide-phase picker, movement/collision and the title/lobby screens have been
   captured and inspected; the seek phase's crosshair/inspect bar and the results screen have not yet been
   captured against a real multi-client round (only unit/integration-tested) — do that before shipping.
