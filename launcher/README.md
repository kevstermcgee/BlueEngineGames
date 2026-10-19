# BlueEngine Games for Windows

The launcher opens maximized by default. Use the standard Windows restore button
to return to a smaller window.

Opening a game starts its guide at the top. Stars on cards and in the guide share
one saved favorites list. The guide header shows the latest release date, or the
installed version's release date when the game is installed. A separate **Installed
on** date records when that release was installed through the launcher; older
installations with no receipt clearly show the date as unavailable. Release history
lists available versions and marks the installed release. **Downloads & checksums**
opens that game's official website for older downloads. A small update button next
to **Play** installs the latest release through the normal download queue. Close a
running game before updating it.

The Tauri 2 launcher provides one library for game installation, launching,
updates and the full game wiki. Discover and filter human-designed or
AI-designed games, favorite games, queue downloads, and use **My library → Update
all** (also available in **Updates**). This checks the latest releases and queues
updates across the entire installed library, including games hidden by filters.
Downloads run one at a time without duplicate queue entries. Running games are
left open; close them and select **Update all** again to update them. Cancel
stops the active download and clears the queue. Installed games launch offline.
The catalog refreshes when the app opens
and with the refresh button; game updates are explicitly started by the player.
The release includes the current game guides and screenshots for offline browsing.
The small chevron above the sidebar switches to an icon rail and remembers your
choice. Navigation, refresh and launcher updates remain available when collapsed.

Each game guide includes a **Feedback center** with an optional display name,
a 1–10 rating, the version played, and writeups for what you enjoyed and what
could be better. **Submit feedback** sends the review directly; no account or
sign-in is required. A blank name appears as **Anonymous**. Names and reviews
are public, so keep private information out of the form.

Drafts stay on the device while offline or after a failed submission. Retrying
the same review uses the same submission ID to avoid duplicates. The launcher
confirms success only after the receiver publishes the review to the engine
repository. Accepted reviews feed future improvement cycles and IdeaForge
planning, building and review. Public ratings and a paginated review browser
remain available in the launcher. The application contains no repository write
credential; publication uses the receiver's server-side Git configuration.

The receiver is deployed separately from the static download site. See the
engine repository's `deploy/feedback/README.md` for HTTPS, Git credentials and
service setup. The frontend and native transport must use the same verified
public endpoint before releasing the launcher. Publication checks the receiver's
health endpoint and stops if it is unavailable.

Installs use `%LOCALAPPDATA%\BlueEngine\Games\<slug>`, sharing receipts with
existing per-game installers. Old `%LOCALAPPDATA%\BlueEngineLauncher\games`
installations are detected too. Games installed in custom folders are not
currently imported. Games do not need separate setup dialogs: bundled games use
verified portable payloads; independent Embertrail and Harvest Hollow releases
use their existing per-user installer in silent mode.
Game installs and updates do not create individual desktop shortcuts. Independent
installers explicitly deselect the desktop shortcut task, including a choice
remembered from an earlier standalone installation.

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
The Windows UI checks use a separate executable built with tauri.ci.conf.json
and the `feedback-smoke` feature. Its feedback endpoint is fixed to loopback, so
synthetic reviews can never reach the production repository. The checks exercise
native submission, a failed request, retry identity and the publication receipt.
The production binary is built without that feature.
The separate executable is needed
because Wry overrides the WebView environment arguments. That executable uses an
older test version and a loopback feed to reject a corrupted signed update, retry
the genuine installer, and verify restart into the exact production binary with
a player file preserved. The production installer is bundled first with
tauri.conf.json, uses HTTPS and never enables a debugging port. Dispatch it with `publish=true` after checks
pass. Each `launcher-v<version>` release is immutable and is **not** GitHub's
latest game release. Publication writes the website download and signed app
update metadata. Bump versions in package.json, package-lock.json, Cargo.toml,
Cargo.lock and tauri.conf.json together for subsequent releases.

Set the repository secret `TAURI_SIGNING_PRIVATE_KEY` to the protected Tauri
updater key. Its public counterpart is in tauri.conf.json. Back up the private
key outside the repository; losing it prevents updating existing installations.
The app checks for launcher updates at startup, hourly, on reconnect and when
returning to the window (with a ten-minute limit for focus checks). Available
updates appear in a banner and the sidebar. **Update & restart** downloads and
verifies the signed package, shows progress, then installs and restarts. **Remind
me next time** hides the banner for the session; the sidebar action stays available.
Game downloads, uninstalls and running games must finish before the launcher can
update. New game operations pause while it updates. Offline checks stay quiet;
failed downloads or verification leave the current launcher usable with a retry.
**Check launcher updates** remains available for manual checks. Keys are never
shipped in the app or repository.
