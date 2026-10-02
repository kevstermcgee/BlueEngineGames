# Feedback on developing with BlueEngine (from building Deadfall)

Ordered by how much each item cost or would save in the next game. "Cost" is time or tokens it actually cost here.

## What worked very well (keep)
* **The netplay kit** (`NetGame` / `ClientView` / `LoopNet`): lobby, sessions, redundant input bundles, exactly-once events, bot takeover when
  someone leaves and per-client snapshots saved weeks of work; 12 players cost 90 kbit/s each. `LoopNet` made lossy-network tests trivial.
* **Headless capture** (`xvfb-run` plus `--capture`): the only way an agent can see its work; every visual defect here was caught this way.
* **`devkit` pieces** (`Rng`, `store_atomic`, `synth::wav_bytes_stereo`, `MenuStep`) and the `kit` renderer got a game on screen in an hour.
* **The shared hub (`netplay::cli::serve` + `netplay::hub`, ADR 0037)**: Deadfall's server main went from 121 lines to 25 and about 2,800 lines of private hub, client, tests and deploy
  units were deleted; the engine's `Online` is Deadfall's old state machine, so the screens needed one constructor change. The legacy adapter plus a
  hand-written copy of the old codec let the shipped clients stay compatible and be tested.
* **The games-repo release workflow**: a new native game needed one manifest line and built on Windows unchanged.

## Highest value fixes
1. **Windows keyboard tracking is a hard-coded list of 23 keys** (`game_input::KeyboardFrame::poll`). With `platform::keyboard()` (which the
   templates pass) `ClientInput::pressed(KeyCode::R)` and G, Tab, V, 1-4 are silently always false. A shooter, a puzzle game with
   hotkeys, anything with a letter bound would ship with dead keys on Windows only. Make the list complete/configurable, or return an error for an
   untracked key, or document it at the top of CUSTOM_SIM_CHEATSHEET. (I bypassed `ClientInput` for gameplay keys.)
2. **No way to test real input.** No key/mouse/controller injection exists, so "do the controls work on a real window" cannot be verified by an
   agent; scripted input bypasses exactly the gate that broke Spooky Kart. An `--inject` layer in `ClientInput` (events from a script, through the real
   path) would close the loop. Same for a virtual gamepad.
3. **No Windows verification path.** No mingw/cross toolchain; a Windows compile error costs a 20-25 minute CI round-trip for the whole release workflow
   (every game builds, so one broken game breaks everyone's release). Per-game dispatch (`workflow_dispatch` with a game input) and a `cargo check
   --target x86_64-pc-windows-gnu` recipe (or `cargo xwin`) would make this cheap.
4. **Audio has no spatial API.** `SoundBank::play(index, volume)`: no pan, pitch or distance. I rendered 7 pre-panned stereo copies of every directional
   sound (154 MB decoded) and every automatic-weapon shot is a bit-identical repeat. Add `play_at(sound, pan, volume, pitch)`; the backend (quad-snd)
   can mix stereo, it is only the wrapper that hides it.
5. **No depth-testing translucent material.** `fx_alpha` ignores depth (miniquad turns the test off when depth writes are off), so glass showed through
   walls across the map; I made it opaque. Also the particle `Fx` are flat squares (no soft sprite), so smoke and dust look like blocks.
6. **Procedural modelling needs a shape toolkit.** `Template` has axis-aligned `box_`, `ball`, `cone`, `cylinder`; every soldier and gun first came out
   blocky and "Lego-like" until two helper agents wrote their own lofts, sweeps, rounded boxes and curved magazines (about 1,500 lines duplicated in
   two files). Shipping `Template::loft`, `sweep`, `rounded_box`, `capsule`, `mirror`, and smooth-normal helpers would have saved about 30% of the art tokens. Also
   consider glTF/OBJ import for art, noted as unsupported today.
7. **Viewmodel pass.** First-person weapons need their own depth buffer or they poke through walls; the engine has no depth-only clear, so I used a
   render target. A `kit::Overlay3d` pass (own camera/FOV, depth cleared) would make every FPS correct by default.
8. **Draw-call capacity trap.** macroquad's default is 10,000 vertices / 5,000 indices per call and meshes beyond it are dropped silently. The engine's
   `game_client::window_config` raises it to 30,000, but my own preview window did not and three helper agents each lost time to missing geometry. Make
   `Template::to_meshes` split to the real capacity or warn once.

## Netplay kit gaps (each needed a workaround)
* `NetGame::start(seed, seats, participants)` is a static fn with no configuration: match rules (kill limit, time, bots) reached it through a process-global.
* One `choice: u8` per seat and no lobby data of your own: team balance (six a side) is enforced at match start and the lobby cannot show "team full" or
  the host's rules. A `lobby_state` extension point would do.
* No joining a match in progress, so a latecomer waits a whole match.
* No lag-compensation helper in the kit (the older multiplayer template has `PoseHistory`); I wrote pose history and rewind (`seen_tick`).
* `NetClient::stats().prediction` reports 0 corrections and identical figures for every client in my run: the view's prediction stats do not seem to reach it.
* The `production` (QUIC/TLS) profile cannot be used by a packaged game: the server needs a private key file that cannot ship, and there is no
  generate-a-certificate path. Direct play therefore uses raw UDP plus a join key, suitable for friends, not the open internet. NAT traversal/relay absent.
* `MAX_SEATS <= 16` and a 1200-byte datagram are fine for 12 players but cap a future 32-player mode.

## Gameplay-programming gaps
* `Controller`: no player-versus-player collision, no stairs-vs-jump info for AI, step height 0.221 m is undocumented in the player-facing docs (I found it
  in source). Gravity and floor defaults (implicit floor at y=0) are traps for maps with pits.
* Navigation: `pathing` is 2D/room-graph; I wrote a 3D multi-floor nav graph (300 lines, plus its stair-tread bug).
* `Lifecycle` (flags, fixed step, capture) is tied to `Sim`/`Held`; a networked game with its own loop has to reimplement `--capture`, `--script`, `--frames`.
  Making these pieces independently usable would help every multiplayer game.
* The scaffold (`be2-tools new-game ... custom-sim`) ships a complete single-player game that has to be deleted; a `--empty` option would save time. Its
  `identity.json` smoke args assume `--exit-after`, which only `Lifecycle` games implement.
* Generated icon art is generic (a letter on a gradient) and `ship.py` is 150 KB of tooling to read when something goes wrong.

## Documentation
* CUSTOM_SIM_CHEATSHEET is excellent for single-player custom sims; add a "networked shooter" page: per-client snapshots, prediction by sharing the
  step function, counters for presses, lag compensation, killcam from a snapshot ring (this document's architecture, in short).
* State the audio limits (mono, one volume) and the 4-light limit up front; both shaped design decisions.

## Small things found moving Deadfall onto the shared hub
* **Settings that exclude each other** (Deadfall's `--kills N` versus `--minutes N`, last one wins in the old server) cannot be said in a `SettingSpec`
  schema; I used `minutes` 0 = "use kills" and `configure` decides. A documented "0 means off" convention for int settings would save the next game the thought.
* **Server flags now refuse out-of-range values** (`--kills 0` is an error) where the old Deadfall server clamped them. Right for a hub, but worth a line in
  `--help` or NETPLAY.md because it is a behaviour change for anyone moving a hand-written server onto `cli::serve`.
* **`ConnectFailure::Unreachable` carries the resolved `SocketAddr`**, so a player who typed a name sees its IP in the message. Carrying the typed text too would read better.
* **`Online` has no "remember this hub" hook**: the game stores `last_used` itself (Deadfall keeps `Prefs::last_hub`, set only for a hub named with `--hub`).
