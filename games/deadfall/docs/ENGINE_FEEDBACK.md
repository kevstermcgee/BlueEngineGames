# Sprint movement follow-up (2026-10-08)

Shift and the left-stick button now request shared-controller sprinting. A new diagonal movement regression exposed weapon-weight and quiet-walk scaling applied before vector normalization; `Input::movement` now normalizes first. The wire round-trip and shared server/prediction movement tests cover sprint, aiming, crouching, quiet walking, weapon weight and smooth return to normal speed. This repair is game-owned and requires no engine API changes.

# Current upgrade findings (2026-10-07)

The final bot planner also exposed synchronous A* spikes above the 12 ms UDP gate. `nav::PathSearch` now resumes at most 1024 heap pops per bot per tick, counts obsolete entries against the budget, rate-limits failed retries and resets on respawn. Map validation retains full searches. This is useful evidence for an engine-owned, multi-floor navigation API with bounded per-tick work (learning record L-089).

This section supersedes the historical observations below. The original report is retained as development history; its measurements and missing features describe that earlier engine.

* **Fixed in the engine:** packet framing allowed 1200 bytes while the transport accepted 1100. A deterministic busy-combat fixture reproduced state starvation and send errors. The encoder now obeys the effective peer limit and encodes game snapshots once per recipient. Deadfall budgets optional nearby projectiles, zones and drops around essential state.
* **Fixed in the engine:** tick acknowledgement lost partial batches of same-tick events. Opt-in reliable events have sequence acknowledgements, bounded retention/reordering, observable gaps, and original event ticks. State continues independently. Oversized events produce a counted gap rather than blocking movement or later events.
* **Fixed in the engine:** delayed lobby messages could reset a new match under reordering. Reliable-event lobby transitions now carry authenticated match generations. Short results screens retry at the usual snapshot cadence so the final combat events can drain. Regression fixtures include repeated matches with 25% loss, event bursts with 30% loss, and oversized events.
* **Adopted current engine input:** the native key table is complete in this engine. Deadfall now uses `ClientInput` for gameplay keys; the historical 23-key workaround is removed. Losing focus clears movement, fire, aim and interactions.
* **Fixed in the game:** Slagworks roof pickups were inaccessible to bots. Match navigation now uses the actual player jump height and checks collision before bridging a narrow roof joint. Reachability checks cover every spawn, weapon and objective on all three maps, and standing spawn physics catches raised-floor penetration.
* **Fixed in the game:** native CI found different raw join fingerprints for identical procedural maps. Numeric inputs are now rounded to 0.0001 units before hashing; one portable fingerprint is pinned on both operating systems (L-090).
* **Fixed in the game:** float-to-u16 conversion saturated the displayed tick after about eighteen minutes, breaking lag compensation. Conversion now wraps through u32, with hit tests before, across and after rollover.
* **Fixed in the game:** DNS lookup blocked the render loop. A single bounded worker now resolves asynchronously; timeout, cancellation and retries remain usable. Numeric socket addresses bypass the worker. A stuck OS lookup cannot accumulate unbounded threads.
* **Verification improved:** Deadfall is included in native Linux/Windows game CI. Real hub tests are mandatory there, with the hub built from the catalog engine. Linux-only process-argument inspection is kept optional on Windows while actual room creation, joining, capacity, mode and map checks run on both.

Remaining engine opportunities grounded in this task: typed per-room context would remove process-global `NetGame` configuration and test serialization; a reusable background resolver could replace game-side glue; shared 3D navigation would remove each game's multi-floor implementation; a reusable native keyboard/mouse/controller fixture would replace one-off input injection. A private Linux XTest review exercised the actual Tab scoreboard key path, while scripted captures covered gameplay and room menus. Legacy custom-sim projects without `game.project.json` still need their native `scripts/check.py`/`ship.py` path rather than automatic `check --game` shipping. None of these gaps prevents this game's current native release.

Transport/measurement limits: hub rooms use UDP without TLS; session ownership is not encryption. Event retention is bounded, so prolonged loss, oversize events or ending results before catch-up can still produce gaps. Loopback and simulated loss are not an interstate connectivity test. Headless software-render captures prove visible output, not hardware frame rate or subjective audio quality. See the current verification report for measured results.

---

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
