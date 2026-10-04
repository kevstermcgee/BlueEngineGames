# BlueEngineGames

Games, prototypes, test content, and demos produced with
[BlueEngine](https://github.com/kevstermcgee/BlueEngine).

**[Browse and download all games](https://kevstermcgee.github.io/BlueEngineGames/)** —
the download site lists every game with a screenshot, description, and direct
download link. It rebuilds automatically from each release
(see `site/` and `.github/workflows/pages.yml`).

**[Download ready-to-play Windows builds](../../releases/latest).** Each game has a
Windows x64 installer that installs for your user, adds Start Menu shortcuts, and
offers a desktop shortcut. No Rust toolchain or command line is required.

Installed games have a **Check for updates** Start Menu shortcut. Updates replace
the existing game and keep saves and settings. You can also run a newer installer
to upgrade the same installation. Open a game's page on the website and use its **Versions** table
to download a specific previous release. Latest is the default.

New games install under `%LOCALAPPDATA%\BlueEngine\Games\<game>`.
The installer recognizes existing installs from the retired launcher and lets you
select an existing portable game's folder. ZIPs remain available for portable use
and for releases published before installers were introduced.

See [DISTRIBUTION.md](DISTRIBUTION.md) for release, versioning, and update details.

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
