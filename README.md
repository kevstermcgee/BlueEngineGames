# BlueEngineGames

Games, prototypes, test content, and demos produced with
[BlueEngine](https://github.com/kevstermcgee/BlueEngine).

Native Windows x64 EXE installers only; no browser play.

**[Browse and download all games](https://kevstermcgee.github.io/BlueEngineGames/)** —
the download site lists every game with a screenshot, description, and direct
download link. It rebuilds automatically from each release
(see `site/` and `.github/workflows/pages.yml`).

Cards and game pages show **AI-designed** or **Human-designed** badges, with a
designer filter alongside the existing library filters. Authored wiki credits
identify the designer before the first publication; IdeaForge publication
commits also identify historical AI-designed games automatically.
Other games are treated as human-designed; this describes the concept and rules,
not whether AI helped write the code. A catalog row may explicitly set
`design_origin` to `ai` or `human` to override that classification. Keep the full
Git history when building the site so IdeaForge credits remain available.

Each game page also has a built-in game guide: overview, rules, controls, story,
characters, screenshots and creation notes. AI designs include their recorded
design rationale; human designs include the original creation request when it
can be recovered. Missing historical originals are clearly labeled.
**New games require a complete guide, three distinct gameplay captures and
character screenshots before publication.** See [guide authoring](site/WIKI_AUTHORING.md)
for the content format and validation commands.

Every game card and detail page uses a real in-game capture from
`site/thumbs/<slug>.png`. New games must include a visually reviewed landscape
screenshot (at least 320 × 180); launcher icons and initial tiles are never used
as substitutes. CI checks screenshot coverage for every release definition, and
a failed screenshot check leaves the previous website intact.

**[Download ready-to-play Windows builds](../../releases/latest).** Each game has a
Windows x64 installer that installs for your user, adds Start Menu shortcuts, and
offers a desktop shortcut. No Rust toolchain or command line is required.

Run a newer installer to upgrade the same installation while keeping saves and
settings. Games built by this repository also have a **Check for updates** Start
Menu shortcut. Open a game's page on the website and use its **Versions** table
to download a specific previous release. Latest is the default.

New games install under `%LOCALAPPDATA%\BlueEngine\Games\<game>`.
The installer recognizes existing installs from the retired launcher and lets you
select an existing portable game's folder. Website downloads are `.exe` installers. Original ZIPs remain as internal update
packages and preserved GitHub release artifacts.

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
