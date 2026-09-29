# BlueEngineGames

Games, prototypes, test content, and demos produced with
[BlueEngine](https://github.com/kevstermcgee/BlueEngine).

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

Run `Install-BlueEngineLauncher.cmd` from a checkout to build the launcher and add
a **BlueEngine Launcher** shortcut to your Windows desktop.

- `games/` contains playable game documents and standalone game crates.
- `prototypes/` contains API and multiplayer starter projects.
- `tests/` contains test maps and runnable validation examples.
- `demos/` contains example scenes and visual previews.

## Physics game experiments

- [`Skyhook Sprint`](games/skyhook-sprint) — matched aerial-momentum course.
- [`Magnet Mine`](games/magnet-mine) — polarity-driven drone docking.
- [`Wrecking Orbit`](games/wrecking-orbit) — spring-tethered wrecking-ball targets.

Each experiment includes deterministic headless tests and an engine-feedback prompt.

On Windows, double-click `Install-Physics-Game-Shortcuts.cmd` to package all three
games and place their launch shortcuts on the Desktop. Each game also has its own
installer if you only want one shortcut.

Most of this repository is an automatically maintained, browsable copy. BlueEngine is
the source of truth for cataloged content. Standalone games explicitly named by the
catalog's `preserved_paths` are maintained here and survive engine synchronization;
other files in the four collection directories are replaced. `.games-catalog.json`
identifies the exact source commit and records a SHA-256 digest for every copied file.
Games, prototypes, tests, and demos produced with BlueEngine.
