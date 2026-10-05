# Slapstick - AI Agent Guide

A game with its own simulation on BlueEngine (path dependency `vesper3d`, see Cargo.toml). Read this
file and `src/lib.rs`. Missing engine capability is engine work, not permission to invent an API: in the
engine checkout run `python tools/be2.py context "<need>"`, and read docs/CUSTOM_CLIENT.md (the kit and
the loop), docs/SHARED_GAMEPLAY.md ("Custom loops") and docs/NETPLAY.md (the lobby/server/client kit).

## Architecture (do not break)
- `src/sim.rs` is the match: pure, seeded (`devkit::Rng`), fixed 60 Hz `Sim::step(&Inputs)`. No window,
  sound device, wall clock or networking. It reaches the window (and, over the network, every client)
  only through `Event`s (`drain_events`). Phase FSM: `Serve { ticks_left, server }` -> `Playing` ->
  (back to `Serve` after a goal, or) `GameOver { winner }`, driven by exact-tick checks so win
  conditions and goals fire at a testable tick, never a frame late.
- **No hidden information.** Unlike Prop Hunt, both players always see identical puck/paddle/score
  state: `AirHockeyGame::snapshot` ignores `participant` entirely (see `netgame.rs`), the mirror image
  of Prop Hunt's redaction table. `tests/net.rs`'s
  `both_clients_snapshot_identical_state_every_tick` is this game's key networking invariant — the
  opposite of Prop Hunt's "never leaks": here the property is "never differs". Do not add a
  per-participant view to `snapshot()` without a real design reason.
- **No rapier3d, deliberately.** Physics is hand-rolled deterministic 2D vector math (circle-circle
  mass-weighted push-out + closing-velocity impulse), adapted directly from `collide_karts` in
  `~/SpookyKart/src/sim.rs`. `~/BlueEngine/docs/adr/0018-physics-save-contract.md` measured that a
  rapier world restored from saved positions (not its internal contact-cache/solver state) diverges
  from the uninterrupted run on the very next tick — exactly what netplay client-side prediction does
  constantly. Do not introduce rapier3d/`physics.rs` into this game without reopening that decision.
- `collide_puck_paddle` only gives the puck an impulse; a human/bot-driven paddle is a kinematic
  target-tracker, not a free body receiving reaction forces. This is a deliberate deviation from
  `collide_karts` (which pushes both bodies), not a bug — do not "fix" it to match.
- `apply_paddle_input` (a free function in `sim.rs`) is the single source of truth for turning a target
  into paddle motion: clamps into the paddle's own half and the table bounds (inset by `PADDLE_RADIUS`),
  moves at up to `PADDLE_MAX_SPEED` per tick. `Sim::step` (authoritative) and `AirHockeyView` (the
  client's own-paddle prediction) both call it, so the two can never diverge in how paddle movement is
  computed. Input is **absolute** (`target_x`, `target_z` normalized `[-1, 1]` within the paddle's own
  half), not a delta, so a resent/duplicated input packet is harmless.
- `src/netgame.rs` is `AirHockeyGame: NetGame` and `AirHockeyView: ClientView`. `start()` assigns seat i
  to paddle i (no shuffling: a 2-seat game has nothing to shuffle). `release()` hands a departed
  player's paddle to the bot AI (`bot::paddle_ai`) rather than leaving a dead match, since with only
  two seats one player leaving would otherwise end the game for the other. `AirHockeyView` predicts
  only the local player's own paddle (replaying `apply_paddle_input` over unacknowledged inputs) and
  interpolates the opponent's paddle and the puck between the last two snapshots.
- `src/bot.rs` is a small paddle-tracks-puck AI reading only the puck's position (the same information a
  real player's screen shows): defend the goal mouth when the puck is in the opponent's half, intercept
  directly when it is in this paddle's own half. Used by `Sim::step` for any `paddle.ai == true` slot,
  by `slapstick-bots`, and by `tests/net.rs`.
- `src/main.rs` is the window: an overhead camera built directly from `kit::View { eye, yaw, pitch,
  roll, fov }` (not `View::first_person`) looking down the table from behind each player's own goal, so
  each client's own paddle reads near the bottom of their own screen. `Screen::{Select, Lobby, Playing,
  Results}` is recomputed every frame from the live snapshot's `Phase`, never from hand-written
  transition edges.
- **Audio has the full music path**, unlike Prop Hunt (which turns music off for a design reason specific
  to that game). `HAS_MUSIC` is `true`: `synth::ambient_spec_for("Slapstick", tagline)` for a
  title-derived ambient loop, `devkit::save::Settings` persisted beside the exe (`music_on`/`sfx_on`),
  an `AudioMenu` with both toggles live, `SoundBank` fed from `settings.music_level()`/`sfx_level()` —
  copied faithfully from `~/BlueEngine/templates/custom-sim/src/main.rs`'s audio wiring, not
  reinvented.
- Cannot see or hear the game? Use `--capture`, `--script`, `--perf` below, and numbers.

## Networking
This game's networking is the structural opposite of Prop Hunt's: Prop Hunt's `snapshot()` is a
per-participant redaction table; this game's `snapshot()` is identical for every viewer, by
construction (it ignores `participant`). Read `src/netgame.rs`'s module doc before changing anything.
- `src/netgame.rs`: wire layouts (`write_input`/`read_input`, `write_snapshot`/`read_snapshot`,
  `write_event`/`read_event`), `start()` (seat i drives paddle i), `release()` (the departed player's
  paddle becomes AI-driven, the match continues).
- `src/bin/slapstick-server.rs`: the dedicated server (`NetServer<AirHockeyGame, _>`). See
  `docs/HOSTING.md` for build/run/deploy (UDP port **4102** — Spooky Kart uses 4100, Prop Hunt 4101; do
  not reuse either).
- `src/bin/slapstick-bots.rs`: bot clients against a real server, for load testing.
- `tests/wire.rs`: every shape round-trips, the datagram-budget and fuzz tests.
- `tests/net.rs`: a full lobby -> match -> result round over an in-memory lossy network, bot-driven on
  both sides, including `both_clients_snapshot_identical_state_every_tick` (this game's key invariant)
  and `release()`'s rule (a departed player's paddle is taken over by the bot, the match continues).
- `tests/udp.rs`: the same server and clients for real, over UDP loopback.

## Save states
- F5 / F9 save and load the `quick` slot in the **solo preview only** (`devkit::snapshot`, wired by
  `devkit::Lifecycle` in `main.rs`); there is no server-side save API for a live networked match yet.
  `impl Snapshot for Sim` in `src/sim.rs` lists everything that decides the match's future. **Every new
  field of `Sim`, `Paddle` or `Puck` goes into `SimState`**, or a save loses it;
  `a_save_from_any_tick_resumes_as_the_game_promises...` fails at the first tick that reads what was
  forgotten, and the error names the `hash_parts` piece.
- `Sim::POLICY` is `SavePolicy::Exact` — hand-rolled vector math, no rigid-body world is involved, so
  this is the strongest and simplest save contract (see "No rapier3d, deliberately" above); never weaken
  it to make a test pass.
- `restore` sanity-checks every position is finite and within table bounds (plus slack), refusing the
  restore rather than silently accepting garbage.
- Loads are all-or-nothing and saves are atomic with a backup; never write your own save file code.

## Checks
- `cargo test` (rules, wire, netplay and determinism, all headless). Add a test for every rule you add.
- `cargo fmt` and `cargo clippy --all-targets` before calling anything done.
- Look at it (requires a display, or `Xvfb`/`xvfb-run` headless):
  `target/debug/slapstick --capture out --frames 30,120 --seed 3 --script "..."`, then
  `python <engine>/tools/contact_sheet.py sheet.png --dir out` (one image instead of many). An
  unattended run (any `--capture`/`--script`/`--exit-after`) skips the title screen straight into a
  local two-paddle (one human, one AI) solo preview when no `--connect`/`server.txt` is configured.

## Definition of done (every game made with BlueEngine)
1. `python scripts/check.py` passes (needs `BE2_TOOLS` pointed at a built `be2-tools`; not available in
   every environment — `cargo test && cargo fmt --check && cargo clippy --all-targets` is the fallback).
   It ends with the ship gate, so it fails until step 2 is done.
2. The game ships as a package with its own icon and desktop shortcut: fill in `assets/identity.json`
   (real title, tagline, controls — already done), regenerate the icon if the title changes
   (`be2-tools icon TITLE assets --replace`), then run `scripts/blue ship`. The catalog copy has its own title-seeded icon set, generated with `be2-tools`.
3. You looked at real frames of the shipped exe and exercised the controls; say what you did not verify.
