# BlueEngineGames

Games, prototypes, test content, and demos produced with
[BlueEngine](https://github.com/kevstermcgee/BlueEngine).

**[Browse and download all games](https://kevstermcgee.github.io/BlueEngineGames/)** —
the download site lists every game with a screenshot, description, and direct
download link. It rebuilds automatically from each release
(see `site/` and `.github/workflows/pages.yml`).

**[Download ready-to-play Windows builds](../../releases/latest).** Every directory
under `games/` has a Windows x64 ZIP containing an `.exe`; extract it and start the
included executable. No Rust toolchain or command line is required. The executables
are not code-signed, so Windows may show a SmartScreen warning.

## Game downloads

- [BlueEngine Launcher](../../releases/latest/download/BlueEngineLauncher-windows-x64.zip) — browse, install, and play every game from one app.
- [BlueEngineSandbox](../../releases/latest/download/blueengine-sandbox-windows-x64.zip)
- [BlueDM](../../releases/latest/download/bluedm-windows-x64.zip)
- [Magnet Mine](../../releases/latest/download/magnet-mine-windows-x64.zip)
- [Riftwake](../../releases/latest/download/riftwake-windows-x64.zip)
- [Skyhook Sprint](../../releases/latest/download/skyhook-sprint-windows-x64.zip)
- [Three Switches](../../releases/latest/download/three-switches-windows-x64.zip)
- [Wrecking Orbit](../../releases/latest/download/wrecking-orbit-windows-x64.zip)
- [Pulse Nova](../../releases/latest/download/pulse-nova-windows-x64.zip)
- [Spooky Kart](../../releases/latest/download/spooky-kart-windows-x64.zip)
- [Dead Air](../../releases/latest/download/dead-air-windows-x64.zip)
- [Clockwork Pinball](../../releases/latest/download/clockwork-pinball-windows-x64.zip)
- [Wobble Tower](../../releases/latest/download/wobble-tower-windows-x64.zip)
- [Tumble Maze](../../releases/latest/download/tumble-maze-windows-x64.zip)

Run `Install-BlueEngineLauncher.cmd` from a checkout to build the launcher and add
a **BlueEngine Launcher** shortcut to your Windows desktop.

- `games/` contains playable game documents and standalone game crates.
- `prototypes/` contains API and multiplayer starter projects.
- `tests/` contains test maps and runnable validation examples.
- `demos/` contains example scenes and visual previews.

## Game downloads and experiments

- [`Physics Lab`](games/physics-lab) — interactive fire, water cooling, buoyancy, mirrors, six materials and changing lighting.
- [`Clockwork Pinball`](games/clockwork-pinball) — neon pinball on real rigid-body physics: CCD steel ball, kinematic flippers, bumpers, lane multiplier.
- [`Wobble Tower`](games/wobble-tower) — stack swinging heavy and light crates under rising wind before the tower topples.
- [`Tumble Maze`](games/tumble-maze) — tilt a board to roll a marble past sinkholes and sweeping bars through three mazes.
- [`Spooky Kart`](games/spooky-kart) — Halloween kart racer for up to eight: eight drivers with their own karts and perks, one haunted map, offline against bots or online on a dedicated server.
- [`Dead Air`](games/dead-air) — flashlight-and-pistol survival horror: keep a relay station broadcasting through the night while a blind, sound-hunting monster stalks the halls.
- [`Pulse Nova`](games/pulse-nova) — high-velocity kinetic arena shooter with chain explosions, jump pads, and wave surges.
- [`Skyhook Sprint`](games/skyhook-sprint) — matched aerial-momentum course.
- [`Magnet Mine`](games/magnet-mine) — polarity-driven drone docking.
- [`Wrecking Orbit`](games/wrecking-orbit) — spring-tethered wrecking-ball targets.

Each experiment includes deterministic headless tests and an engine-feedback prompt.

On Windows, double-click `Install-Physics-Game-Shortcuts.cmd` to package the
games and place their launch shortcuts on the Desktop. Each game also has its own
installer if you only want one shortcut.

Most of this repository is an automatically maintained, browsable copy. BlueEngine is
the source of truth for cataloged content. Standalone games explicitly named by the
catalog's `preserved_paths` are maintained here and survive engine synchronization;
other files in the four collection directories are replaced. `.games-catalog.json`
identifies the exact source commit and records a SHA-256 digest for every copied file.
Games, prototypes, tests, and demos produced with BlueEngine.
