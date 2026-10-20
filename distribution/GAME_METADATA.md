# Portable game metadata in Windows downloads

New packages carry the same `blueengine.game` version 1 document in their owned
playable EXEs, top-level BAT/CMD launchers, ZIP and freshly compiled Inno installer.
The ZIP contains a stored root `blueengine.game.json` and the same bounded
metadata envelope in its comment. This checksum protects the document's bytes;
it does not authenticate a creator or authorize publication.

`game_metadata.py` is vendored byte-for-byte from BlueEngine commit `5d3ce7d`:

```text
SHA256 d858cab3d524de4dba1f69c2f959a10adbfb18cb3300046affe340c288c73936
```

Keeping the reader here lets packaging use the established catalog Engine SDK
pin. Update the Engine and Games copies together when the format changes. The
launcher has a bounded reader for this same format; inspection never runs the
game, installer or batch file.

`package_metadata.py` freezes source identity before temporary Cargo SDK binding.
Engine-owned exports use the catalog commit and original export path. Companion
games use their committed GitHub source. Game controls, public author names,
version, tags and available wiki information travel with those pins. Uncommitted
game or Engine source cannot acquire a committed source claim.

Packaging stamps owned outputs before hashes are computed. Installer packaging
preserves the manifest and restores the ZIP comment after adding the updater.
Existing producer hashes are checked before stamping. Only hashes of changed
carriers and the new manifest are refreshed; verification of the previous bytes
is cleared while source observations remain intact.
The Windows review compares all carriers and final hashes, installs one actual
game installer, and verifies its installed metadata. Existing startup checks run
the metadata-bearing game binaries.

Immutable external executables are kept byte-identical. Feta receives a freshly
compiled metadata-bearing Play wrapper; its document explicitly marks editable
source unavailable. Historical backfills preserve their old ZIP and mark missing
source and carrier metadata explicitly in the new installer. Independently
published legacy installers outside this builder gain metadata when their own
source release pipeline adopts the format; old immutable downloads are retained.

Inspect a download without executing it:

```sh
python distribution/game_metadata.py inspect path/to/game.exe
python distribution/game_metadata.py inspect path/to/game-windows-x64.zip
python distribution/package_metadata.py audit --dist dist
```
