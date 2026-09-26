# BlueEngineGames

Games, prototypes, test content, and demos produced with
[BlueEngine](https://github.com/kevstermcgee/BlueEngine).

**[Download ready-to-play Windows builds](../../releases/latest).** Extract a ZIP
and double-click its `Play-*.exe` launcher; no Rust toolchain or command line is
required. The executables are not code-signed, so Windows may show a SmartScreen
warning.

- `games/` contains playable game documents and standalone game crates.
- `prototypes/` contains API and multiplayer starter projects.
- `tests/` contains test maps and runnable validation examples.
- `demos/` contains example scenes and visual previews.

Most of this repository is an automatically maintained, browsable copy. BlueEngine is
the source of truth for cataloged content. Standalone games explicitly named by the
catalog's `preserved_paths` are maintained here and survive engine synchronization;
other files in the four collection directories are replaced. `.games-catalog.json`
identifies the exact source commit and records a SHA-256 digest for every copied file.
Games, prototypes, tests, and demos produced with BlueEngine.
