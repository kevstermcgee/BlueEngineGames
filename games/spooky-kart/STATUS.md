# Spooky Kart Status

## Completed
- Project scaffolded from BlueEngine's `custom-sim` template; design decisions in `DESIGN.md`.
- **Milestone 1, the pure simulation** (`src/lib.rs` and modules, 19 tests in `tests/race.rs`):
  Haunted Hollow track, arcade kart physics (throttle, steering, drift boost, grass, walls, bumping),
  eight characters with unique stats and perks, hazards, deterministic bots, 3-lap race with countdown,
  standings and finish rules, save states (exact), and `RaceReport` telemetry per race.

- **Milestone 2, the window** (`src/main.rs`, `src/models.rs`, `src/controls.rs`): character select with a
  podium and stat bars, chase-camera race, HUD (position, lap, standings, minimap, speed, perk cooldown,
  countdown), results screen, effects and sound from the engine kit, F5/F9 quick save/load. Eight distinct
  kart-and-driver models, Haunted Hollow scenery (road, kerbs, glowing barriers, gravestones, dead trees,
  jack-o'-lanterns, lantern posts, start arch). **Controller support**: right trigger or A gas, left trigger
  brake, left stick steer (with a response curve), bumpers drift, X perk, D-pad/stick and A in menus, Start
  pauses; the mapping is the pure `controls::resolve` with unit tests. Not verified on real hardware.
- **Milestone 3, multiplayer** (`src/wire.rs`, `server.rs`, `client.rs`, `transport.rs`, `bin/spooky-kart-server.rs`):
  compact binary protocol, authoritative server with lobby/race/results, join key, bot takeover when a player
  leaves, client-side prediction and interpolation, engine QUIC/TLS available as `--transport production`.
  Window: `--connect`, or a `server.txt` next to the exe, with offline practice as the fallback (press O).
  Tests: `tests/wire.rs` (codec, fuzz), `tests/net.rs` (10 scenarios on an in-memory network with delay and
  loss, bots as clients), `tests/udp.rs` (real sockets). Verified with the real server and the real window
  client together on a virtual display.
- **Milestone 4, data** (`tools/analyze.py`, `tools/load_test.py`, `bin/spooky-kart-bots.rs`): every race appends
  to `matches.jsonl`; the load test races bot clients over real UDP; BlueEngine's `python tools/perf.py record
  --suite kart` records it as `kart_*` metrics. First baseline (1, 4 and 8 bot clients, dev build): the server
  used 1.4-1.9% of one core and 3.7 MB, tick 130-170 us mean, 18 KB/s down and 2.2 KB/s up per client, RTT 17 ms
  on loopback, no prediction snaps. Lessons for the engine: `docs/ENGINE_LESSONS.md`.
- **Milestone 5, shipping**: the release package (`python3 scripts/ship.py package`, 7.4 MB) and the desktop
  shortcut (`python3 scripts/ship.py shortcut`) build on Linux and `python3 scripts/check.py` passes including the
  ship gate (`scripts/blue` needs a `python` binary, which this box lacks; call the Python scripts directly).
  The package carries a comments-only `server.txt`. Published into BlueEngineGames as `games/spooky-kart` (pull
  request; the engine's `games-publish.json` preserves it) with a Windows release definition.
  **To let friends connect with one click, put the public `host:port` (and join key) in
  `games/spooky-kart/server.txt` in BlueEngineGames before a release.**
- Race length is now 2 laps (about 1:40 with bots).
- **Shadows** (presentation only: no rules, physics, bots, netcode, save hash or content fingerprint touched).
  Esc > Settings > Shadows cycles Off / Simple / Full, remembered in `settings.json`; `--shadows off|simple|full`
  overrides it for one run. Simple (the default) puts a soft contact blob under every kart and hazard; Full adds
  the engine's single shadow map of the moon around the human's kart (32 m half-width, 2048 texels, centred 12 m
  ahead, strength 0.85). `models::world` splits the hollow into receivers (ground slab, road, verges, kerbs: never
  drawn into the shadow pass) and casters (walls, arch, scenery, grouped in 64 m cells that the pass skips when far
  from the focus). The moon (`halloween_look`) is lower and more from the side so shadows read.

## Seeing the game without a display
This box is headless. Use a virtual display and the engine's capture flags; software rendering is slow (a full
race takes about 7 minutes), so capture few frames:
`LIBGL_ALWAYS_SOFTWARE=1 xvfb-run -a -s "-screen 0 1280x720x24" target/debug/spooky-kart --capture DIR --frames 30,300 --exit-after 320 --character ghost --autopilot --size 1280x720 --mute`
(`--select` captures the character select screen; `--script` drives the human's input; `DIR` must not exist.)
Pitfall found: the engine silently skips any template over 9,000 vertices, so the world is built in chunks
(`models::Chunks`); a scene that renders as an empty void means a template got too big.

## Balance data (bots only; `cargo test --no-default-features --test race every_character -- --nocapture`)
24 races, rotating grid. After one tuning pass, average finish times are within about 3 s of each other
(160-163 s for three laps) and wins are spread 0-7 per character. Before tuning, Frankenstein's Monster won
11 of 24 and Zombie, Ghost and Clown won none. Bots follow one racing line and rarely use drift or
handling, so this only guards against a broken kart; real tuning needs human play. A race takes about
160 s: consider fewer laps or a shorter track.

## Next Steps
- Verify controller feel on real hardware; tune handling and the camera by playing.
- Deploy the server on the Debian box (deploy/spooky-kart-server.service, docs/HOSTING.md); prepared, not yet enabled.
  The production QUIC/TLS transport is verified end to end with a real certificate.
- Predict kart-to-kart collisions on the client (see docs/ENGINE_LESSONS.md, item 5).
- Milestone 5: identity and icon, `scripts/blue ship`, publish to BlueEngineGames.
