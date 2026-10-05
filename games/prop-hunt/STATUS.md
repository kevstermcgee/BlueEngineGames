# Prop Hunt Status

## Completed
- Project scaffolded from BlueEngine's `custom-sim` template, copying Spooky Kart's and Dead Air's own
  boilerplate (Cargo.toml shape, `build.rs`, `platform.rs`, `rustfmt.toml`, `.gitignore`, `scripts/*`).
- **The simulation** (`src/sim.rs`, `src/layout.rs`, `src/disguise.rs`, 39 tests across `src/*` and
  `tests/determinism.rs`): the hide/seek phase FSM (exact-tick win conditions), hold-to-inspect tagging,
  confirm-at-a-legal-spot placement with end-of-hide-phase auto-placement (nobody is ever left
  undisguised), Hollow Pine's six-room house with 14 legal disguise spots and matching decoy props, save
  states (`SavePolicy::Exact`).
- **Networking** (`src/netgame.rs`, `src/bot.rs`, `src/bin/prop-hunt-server.rs`,
  `src/bin/prop-hunt-bots.rs`, 17 tests in `tests/{wire,net,udp}.rs`): `PropHuntGame: NetGame` with a
  genuinely per-participant `snapshot()` — the first game in this family to use that plumbing for real
  information hiding rather than identical broadcast. The critical property (an untagged hider's client
  never receives another live hider's real position or disguise) is proved against real `Sim` state, not
  just round-tripped. One seeded-random seeker per round among whoever preferred it in the lobby; no bot
  AI fills an empty hider seat or replaces a departed player (see `AGENTS.md`).
- **The window** (`src/main.rs`, `src/models.rs`): title/lobby/hide/seek/results screens, a disguise
  picker with live confirm-radius feedback, a seeker inspect-progress bar, a local solo preview for
  offline testing (always hider seat 0 — there is no bot AI to drive a solo seeker). Sound effects only,
  no music (a deliberate decision: silence is the tension mechanic; see `AGENTS.md`).
- Headless captures (`Xvfb`) confirm the title screen, house geometry, collision and the disguise picker
  all render correctly; see `AGENTS.md`'s "Definition of done" for what has *not* yet been captured (the
  seek-phase HUD and results screen against a real multi-client round).

## Known limitations (flagged deviations from the original plan, not oversights)
- `Sim` stores a live `vesper3d::viewer::controller::Controller` per hider/seeker, not the
  `ControllerState` the plan's illustrative struct sketch showed; `HiderState`/`SimState` (the save-file
  shape) do use `ControllerState`, matching Dead Air's and Spooky Kart's own `Sim`/`SimState` split.
- A disguise's render mesh (`models::disguise_mesh`) is hand-built from `kit::Template` primitives sized
  by the real catalog `half_extents`, not the engine's authored prop art (`props::scene`) — consistent
  with how both sibling games build every mesh themselves, and avoids coupling this game to the stock
  room-rendering pipeline the plan explicitly said not to reuse (`prop_physics::PropBody`).
- No snapshot-to-snapshot interpolation for remote entities (`ClientView::frame` is a no-op beyond this
  client's own predicted entity): acceptable since most entities are either stationary (a confirmed
  hider) or move slowly, but a visible simplification worth knowing about before polishing camera feel.
- The icon assets in `assets/` are a placeholder, copied from Dead Air's, because `be2-tools` (needed to
  generate Prop Hunt's own icon) was not available in the environment this game was built in.

## Seeing the game without a display
This environment is headless. Use a virtual display and the engine's capture flags:
`xvfb-run -a target/debug/prop-hunt --capture out --frames 30,150 --exit-after 170 --seed 3 --script "fwd:0-140,right:0-140,confirm@150"`
(an unattended run skips the title screen into a local solo preview automatically; `--script` accepts the
vocabulary in `main.rs`'s `CUES`, including `confirm` and `inspect`; `out` must not already exist).

## Next Steps
- Deploy the dedicated server (`deploy/prop-hunt-server.service`, `docs/HOSTING.md`; UDP port **4101** —
  Spooky Kart already uses 4100 on the same box) — prepared, not yet enabled. Production QUIC/TLS reuses
  Spooky Kart's certificate convention (`feta.local`) but has not yet been verified end to end for this
  game specifically (only development UDP has: `tests/udp.rs`).
- Capture and inspect the seek phase (crosshair, inspect-progress bar, a tagged hider's spectator view)
  and the results screen against a real multi-client round, not just unit/integration tests.
- Regenerate `assets/icon*` with this game's own art (`be2-tools icon "Prop Hunt" assets --replace`) once
  `be2-tools` is available, replacing the current Dead Air placeholder.
- Real playtesting to tune `HIDE_PHASE_TICKS`/`SEEK_PHASE_TICKS`/`INSPECT_HOLD_TICKS`/`INSPECT_RADIUS`:
  current values are first guesses (30 s hide, 3 min seek, 0.6 s hold at 2.2 m), never played by a human.
- `scripts/blue ship` has not been run (needs `be2-tools`/a built native binary for `scripts/check.py`'s
  ship gate; not available in the environment this game was built in).

## Catalog packaging validation (2026-10-04)

The BlueEngineGames copy now has its own title-seeded icon and current package tooling.
Rules, saves, wire encoding and UDP loopback tests passed against the current engine,
as did Clippy. A release package and private shortcut passed verification, with two
nonblank captures from a clean staged package on an isolated virtual display.
Two packaged clients completed a real local UDP hide/seek round; the lobby,
seek crosshair, hiders-win result and return to the lobby were captured and inspected.
Human controls, subjective audio and online tuning remain unverified playtest work.
