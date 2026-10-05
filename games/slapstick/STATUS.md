# Slapstick Status

## Completed
- Project scaffolded from Prop Hunt's boilerplate (the more recently-touched sibling scaffold:
  `Cargo.toml` shape, `build.rs`, `platform.rs`, `rustfmt.toml`, `.gitignore`, `scripts/*`).
- **The simulation** (`src/sim.rs`, 20 of 23 total unit tests, plus 14 in `tests/determinism.rs`):
  hand-rolled deterministic 2D vector physics (no rapier3d, per
  `~/BlueEngine/docs/adr/0018-physics-save-contract.md` — see `AGENTS.md`), a
  `Serve -> Playing -> GameOver` phase FSM with exact-tick goal detection and scoring, sudden death past
  the 8-minute soft cap, and `SavePolicy::Exact` save states with a bounds-checking `restore`.
- **Networking** (`src/netgame.rs`, `src/bot.rs`, `src/bin/slapstick-server.rs`,
  `src/bin/slapstick-bots.rs`, the remaining 3 unit tests plus 12 in `tests/{net,udp}.rs` and 7 in
  `tests/wire.rs`): `AirHockeyGame: NetGame` with a
  `snapshot()` that ignores `participant` entirely — this game has no hidden information, the structural
  opposite of Prop Hunt's per-participant redaction table. The critical property
  (`both_clients_snapshot_identical_state_every_tick`) is proved against a real loopback match, not just
  round-tripped. A departed player's paddle is handed to the bot AI (`bot::paddle_ai`, reading only the
  puck's position) rather than ending the match — a deliberate departure from Prop Hunt's "no AI" choice,
  since with only two seats one player leaving would otherwise strand the other.
- **The window** (`src/main.rs`, `src/models.rs`): a fixed overhead camera built directly from `kit::View`
  looking down the table from behind each player's own goal; mouse motion (or the left stick, absolute,
  when pushed) drives a persistent paddle cursor through the same `apply_paddle_input` the server uses;
  score/serve-countdown/goal/win-lose HUD; the full persisted-settings ambient-music path from the
  `custom-sim` template (unlike Prop Hunt, which turns music off for a reason specific to that game).
- Headless captures (`Xvfb`) confirm the table, puck, both paddle colours, the score panel, the serve
  countdown bar, and — via a scripted run that drives an actual goal — the "GOAL!" banner and the score
  updating live all render correctly from the overhead camera.

## Known limitations (flagged deviations, not oversights)
- The icon assets in `assets/` are a placeholder, copied from Prop Hunt's (itself copied from Dead Air's),
  because `be2-tools` was not available in the environment this game was built in.
- No snapshot-to-snapshot interpolation tuning beyond the straightforward lerp in `AirHockeyView::refresh`
  (opponent paddle and puck lerp between the last two snapshots, with light velocity extrapolation on a
  late packet); never played over a real, laggy internet connection to tune `SNAP_DISTANCE`/extrapolation
  feel.
- `apply_paddle_input`'s default (no input) position — the midpoint between a paddle's own backline and
  the centre line — was chosen for a reasonable idle stance, not tuned against real play.

## Seeing the game without a display
This environment is headless. Use a virtual display and the engine's capture flags, e.g.:
`xvfb-run -a target/debug/slapstick --capture out --frames 30,90 --exit-after 100 --seed 3` (an
unattended run skips the title screen into a local solo preview — paddle 0 human, paddle 1 bot AI —
automatically). To watch an actual goal, drive the paddle cursor from a script, e.g.
`--script "aim:0/-1@0-59,aim:0/1@60-300"` (seed 0 serves from paddle 0 first).

## Next Steps
- Deploy the dedicated server (`deploy/slapstick-server.service`, `docs/HOSTING.md`; UDP port **4102** —
  Spooky Kart uses 4100 and Prop Hunt uses 4101 on the same box) — prepared, not yet enabled. Production
  QUIC/TLS reuses the shared `feta.local` certificate convention but has not yet been verified end to end
  for this game specifically (only development UDP has: `tests/udp.rs`).
- Regenerate `assets/icon*` with this game's own art (`be2-tools icon "Slapstick" assets --replace`) once
  `be2-tools` is available, replacing the current Prop Hunt placeholder.
- Real playtesting: two actual humans over a real network, to tune `PADDLE_MAX_SPEED`,
  `PADDLE_PUCK_RESTITUTION`, `WALL_RESTITUTION` and the camera's distance/pitch — current values are first
  guesses tuned only against headless captures and bot-vs-bot matches, never played by a human.
- `scripts/blue ship` has not been run (needs `be2-tools`/a built native binary for `scripts/check.py`'s
  ship gate; not available in the environment this game was built in).

## Catalog packaging validation (2026-10-04)

The BlueEngineGames copy now has its own title-seeded icon and current package tooling.
Rules, saves, wire encoding and UDP loopback tests passed against the current engine,
as did Clippy. A release package and private shortcut passed verification, with two
nonblank captures from a clean staged package on an isolated virtual display.
Human controls, subjective audio and online tuning remain unverified playtest work.
