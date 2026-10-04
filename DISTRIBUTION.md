# Game distribution

Download a game's `*-setup-windows-x64.exe` from the website or GitHub Releases.
The installer uses `%LOCALAPPDATA%\BlueEngine\Games\<slug>` by default, needs no
administrator rights, and creates Start Menu play and **Check for updates**
shortcuts. A desktop shortcut is optional. Windows Apps settings can uninstall it.
The BlueEngine Launcher application has been retired.

Run **Check for updates** with the game closed. The game's updater checks the
latest `Games-catalog.tsv`, compares the installed release receipt, downloads the
exact release's portable package, and verifies its SHA-256. It validates the
playable executable and prepares the update beside the existing installation.
Player-created files, including saves and settings, are copied forward. Unchanged
obsolete package files are removed; modified obsolete files are retained. The old
installation is moved aside only after preparation succeeds and is restored if
the final rename fails. Updates do not require uninstalling or choosing a new folder.

The first installer can upgrade an old launcher-managed installation in place:
it detects `%LOCALAPPDATA%\BlueEngineLauncher\games\<slug>`. For an old extracted
ZIP, select that game's existing folder in the installer. Old packages lack an
update helper, so this first migration uses the installer. Later updates use the
per-game shortcut or a newer installer. An unversioned installation can be updated.
Portable ZIP users can run the included `Update-<slug>.exe`.

Updates keep save files; compatibility with a changed game remains governed by
BlueEngine's save-format/content checks. Uninstallers remove installer-owned files
and shortcuts while retaining player-created saves and settings.

## Releases and history

The release workflow pins the engine revision from `.games-catalog.json` and builds
the games checkout. A release tag contains both engine and games revisions, plus
the workflow run and attempt. Game-only changes and rebuilt packages therefore
produce distinct releases. Existing releases are never overwritten. Asset names
are stable within each release; catalog URLs point to that specific tag.

`distribution/build_catalog.ps1` generates `Games-catalog.tsv` with slug, title,
description, creation date, game and engine versions, package kind, exact ZIP URL,
release identity, and SHA-256. The updater compares release identity rather than
Cargo's version string, which games sometimes leave unchanged. The installer puts
`.installed-release` and `.installed-files.tsv` beside the executable. The latter
records shipped paths and hashes for safe replacement; these local receipts are
excluded from update ZIPs. The portable package includes the per-game updater.
`SHA256SUMS.txt` covers the final ZIPs, installers, and catalog.

The website fetches all pages of published GitHub releases at build time. Latest
is shown by default, and each game's **Older versions** section links to actual
assets from each previous release. Installers are preferred when present; older
ZIP-only releases stay downloadable. Checksums belong to the selected release.
Drafts, prereleases, and releases without that game are excluded. Historical tags
are shown even when two releases use the same semantic game version.

The website temporarily accepts the old catalog filename until the first new
release finishes. It provides no launcher download. Existing historical release
artifacts remain intact.

`non_playable_directories` in `.release-games.json` explicitly identifies source
fixtures that are not game products. The Observatory authoring/audio fixture is
excluded this way; every other game directory must still have a release definition.

## Validation

`distribution/test_updater.ps1` exercises migration, detection, player files,
obsolete-file pruning, checksum rejection, invalid packages, directory traversal,
and rollback after a failed commit. `distribution/test_installers.ps1` builds and
runs actual installers to check installation, upgrade, shortcuts, save retention,
and uninstallation. Both run on Windows before game builds in the release workflow.
`python -m unittest discover -s site -p 'test_*.py'` checks download-history behavior.
Branch dispatches build reviewable Windows artifacts without publishing releases.
