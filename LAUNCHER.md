# BlueEngine Launcher

`BlueEngineLauncher.exe` automatically lists every first-level game under `games/`.
Each card shows its creation date, game version, pinned BlueEngine revision, game
type, and description. **Install & play** downloads the matching ZIP from the latest
BlueEngineGames release into the current user's local application-data directory,
then starts its `.exe`.

Run `Install-BlueEngineLauncher.cmd` to build the launcher, install the launcher app
under `Documents/Codex/Launchers`, and create a desktop shortcut. Downloaded games
remain in the current user's local application-data directory. The release workflow also publishes
`BlueEngineLauncher-windows-x64.zip`.

The launcher catalog is generated from `.games-catalog.json`,
`.release-games.json`, repository history, and the game directories. A new game
therefore appears automatically as soon as it has the release definition required
by the repository's all-games coverage gate.

## Local games and catalog merging

The launcher never lets the online catalog remove a game you already have. The
catalog downloaded from the latest release is only cached (as `remote-catalog.tsv`
in the per-user launcher folder, `%LOCALAPPDATA%\BlueEngineLauncher`); the
`launcher-catalog.tsv` next to the launcher is left untouched. On every load the
shipped catalog, the cached remote catalog and the installed games are merged by
slug: remote fields win for games that exist remotely, and games that exist only
locally are kept.

The launcher also scans `%LOCALAPPDATA%\BlueEngineLauncher\games\*` on load. A
subfolder that contains `<slug>.exe` (or `Play-<slug>.exe`, or failing that a single
`.exe`) is listed as an installed game even with no catalog row, so a game you
build and drop into that folder appears automatically. Its name and description
come from an optional `game.json` (`name`/`title`, `description`, `version`),
`manifest.tsv` (`key<TAB>value` lines) or `ship.json` (`title`); otherwise the name
is the slug in title case and the kind is "local game". Local games have no download
URL, so the button only plays them and never tries to download.
