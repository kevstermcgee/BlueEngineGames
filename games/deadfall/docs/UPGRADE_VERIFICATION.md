# Deadfall upgrade verification — 2026-10-07

## What changed

The native game retains its existing visual style and server-authoritative 60 Hz simulation. Slagworks gains spawn cover and new pickups. Switchyard adds compact freight-yard lanes and loading overlooks; Stormbreak adds coastal relay towers, generators and elevated approaches. All three have collision-aware paths to every spawn, weapon, flag base and bomb site.

Team Deathmatch and Free for All support a two-human duel with opposing teams where applicable, no automatic bots and a two-seat room cap. Capture the Flag requires returning the enemy flag while the home flag is present; dropped flags return after twenty seconds. Search and Destroy alternates attacking sides, has no respawns during a round, and requires continuous three-second planting or five-second defusing. A planted bomb continues after the attacker dies. The first configured capture/round target wins.

Hornet is a burst sidearm, Ranger a scoped cycling marksman rifle, and Breach-8 a slug shotgun. Existing weapon IDs stay fixed; the new IDs are 34–36. Rifleman, Scout, Recon and Breacher share the existing skeleton and hitboxes, with four skin options per team. Faster movement, higher vaultable jumps and four-second deathmatch respawns reduce downtime.

## Network and engine evidence

The engine upgrade fixes a reproduced mismatch between the encoder's packet limit and the active 1100-byte transport budget. Deadfall opts into independent sequenced combat events, bounded retransmission and reordering, original event timestamps, session tokens and match epochs. Tests include burst traffic, 30% loss, delayed/reordered traffic, oversized events and repeated matches. Existing games retain their original wire behavior.

The game snapshot prioritizes the essential roster, inventory and objectives before nearby transient effects. Real hub integration exercises all eight stable settings, all four modes, requested maps, replicated cosmetics, one-player waiting and refusal of a third duel client. Old discovery clients receive an update notice; game protocol 2 requires both friends and the live server to update. Procedural numeric fingerprint inputs are rounded to 0.0001 units so harmless math-library roundoff does not reject another native OS. Lag-compensation ticks wrap instead of saturating after eighteen minutes.

Map navigation is prepared before the server accepts players. Long bot searches are resumed with a budget of 1024 heap pops per bot per tick, including obsolete queue entries; failed paths are rate-limited and respawns discard old plans. This repairs observed 20–26 ms search/startup spikes without loosening the 12 ms timing gate. Two focused UDP runs after the bounded planner measured 4.40 and 4.66 ms worst server ticks on this machine. These are short fixtures, not a guarantee under arbitrary system load. Bot navigation fixtures complete flag captures and bomb planting on every map; competitive bot combat has separate multi-seed soaks. An isolated navigation fixture does not promise that two competing bot teams avoid stalemates.

## Verification receipts

* [Native engine Linux/Windows CI](https://github.com/kevstermcgee/BlueEngine/actions/runs/37701469367) passed all six jobs at `f0657a1b3ee7`, including full checks, release/headless builds and existing game consumers.
* [Native Deadfall Linux/Windows checks](https://github.com/kevstermcgee/BlueEngineGames/actions/runs/37707584354) passed at `1bfc5f65f15b`: 132 library and 34 integration tests per operating system, with three deliberately ignored library tests. The real shared-hub tests ran on both. Formatting and Clippy with warnings denied passed. Both platforms package the game; Linux also runs the isolated clean-package smoke. The [selected Windows installer review](https://github.com/kevstermcgee/BlueEngineGames/actions/runs/37707587035) passed distribution, updater and installation/upgrade/uninstall gates, and its downloaded installer/package match the SHA-256 manifest. This branch artifact is separate from the eventual published release.
* The full local engine check passed all 39 gates in 2540.480 seconds; 40 stage/total timing rows are stored in the engine's `docs/perf/metrics.jsonl`. Learning-data validation passed 52 tests after the fixed network entries were marked promoted.
* Local native project checks passed with a real hub; the rendering-free library passed 64 tests with two deliberately ignored. Timing gates retain their original threshold. The protocol-2 raw join pin is `fed4da58` on both native OSes.
* Native render review covered the three maps from three overhead angles, all four character variants, the three new weapon models and sights, gameplay in all four modes, settings, the main menu and online room creation/lobby. It caught and repaired floor depth flicker and owner-camera flag obstruction. Bomb-site labels follow visible world markers. A private Linux XTest run confirmed that holding the actual Tab key displays the scoreboard; this is not a claim of physical controller testing.
* The native friend-room capture created a two-human room, marked the lone player Ready, confirmed that it remained in the lobby and exercised Copy invite. The server candidate passed registry/schema validation and isolated startup with all eight room settings.

Publication uses the main-branch Windows release and Pages workflows. The catalog pins the complete engine revision; release manifests and native `ship.json` receipts identify the exact packaged source. Both players must update before joining the protocol-2 server.

## Practical limits

Loopback and simulated packet-loss tests establish behavior under their fixtures; they do not establish interstate routing, Wi-Fi quality or a particular player's latency. The shared public hub currently uses UDP without TLS. Reliable combat-event retention is bounded to 4096 events, 1 MiB and ten seconds; explicit gaps allow recovery if that window is exhausted. Software-rendered review captures establish visibility and layout, not hardware frame rate or subjective weapon feel. Playing a full match with the cousin remains useful feedback after release.

## Reproduce

Run `cargo fmt --all --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked --no-default-features`. Set `BE2_HUB` to the matching engine's built `be2-hub` so actual room tests are mandatory. Run the game's `scripts/check.py` and `scripts/ship.py ship` for native identity/package checks. The catalog CI runs game checks on native Linux and Windows; Windows publication builds the installer.

Engine feedback lives in `docs/ENGINE_FEEDBACK.md` and the engine's `docs/feedback/2026-10-07-deadfall-networking.md`, with regression suites and retrievable learning records L-086 through L-090.
