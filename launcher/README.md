# BlueEngine Games for Windows

The Tauri 2 launcher provides one library for game installation, launching,
updates and the full game wiki. Discover and filter human-designed or
AI-designed games, favorite games, queue downloads, and use **Updates → Update
all**. Installed games launch offline. The catalog refreshes when the app opens
and with the refresh button; game updates are explicitly started by the player.
The release includes the current game guides and screenshots for offline browsing.

Installs use `%LOCALAPPDATA%\BlueEngine\Games\<slug>`, sharing receipts with
existing per-game installers. Old `%LOCALAPPDATA%\BlueEngineLauncher\games`
installations are detected too. Games installed in custom folders are not
currently imported. Games do not need separate setup dialogs: bundled games use
verified portable payloads; independent Embertrail and Harvest Hollow releases
use their existing per-user installer in silent mode.

Downloads use immutable GitHub release URLs, a byte length and a SHA256 from the
published catalog. ZIP paths, symlinks, Windows device names, duplicate paths,
size limits and reserved receipts are validated before atomic replacement. A
per-game lock coordinates with existing updaters. Extra player files and modified
obsolete package files are preserved; unchanged obsolete files are removed.
Uninstall retains player-created files. Close a game before updating it.
A cancelled or failed ZIP preparation leaves the installed game available.
Independent installers retain their own upgrade and rollback behavior.

The app uses a local bundled UI with restricted Tauri commands, never remote
HTML with native privileges. No administrator rights are required. Windows
SmartScreen reputation and Authenticode signing are separate from Tauri update
signatures. The initial installer is unsigned; Windows may also check individual
game executables. The launcher does not suppress security protections or promise
a single warning. A trusted Windows signing identity can be added to future
release builds when available.

## Development and releases

On Windows with Rust, the Visual Studio C++ build tools and Node 22:

```
cd launcher
npm ci
npm test
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --locked
npm run tauri -- build --bundles nsis
```

Core transaction tests also run on Linux without GTK using
`cargo test --no-default-features` from `src-tauri`. `npm run build` makes a
browser preview; native actions display a Windows-app notice in that preview.
The checked-in `public/catalog.json` is the initial offline snapshot. The app
refreshes from the website's generated `catalog.json`, which includes new games
automatically. Before a release, refresh the snapshot with the audited site
builder and copy its catalog. `build.mjs` bundles real game screenshots from the
repository for the offline guide snapshot.

The **Build Windows launcher** workflow verifies tests, downloads and installs a
real published ZIP game and both independent installers in temporary folders, builds the per-user NSIS installer,
and checks the real Windows WebView renders the library, searches guides, installs
a game through native IPC and uninstalls it while keeping a player-created save.
The CI-only WebView debugging argument is never enabled in app settings. Dispatch it with `publish=true` after checks
pass. Each `launcher-v<version>` release is immutable and is **not** GitHub's
latest game release. Publication writes the website download and signed app
update metadata. Bump versions in package.json, package-lock.json, Cargo.toml,
Cargo.lock and tauri.conf.json together for subsequent releases.

Set the repository secret `TAURI_SIGNING_PRIVATE_KEY` to the protected Tauri
updater key. Its public counterpart is in tauri.conf.json. Back up the private
key outside the repository; losing it prevents updating existing installations.
The app checks signed launcher updates with **Check launcher updates** and offers an install button before restarting after installation. Keys are never shipped in the app or repository.
