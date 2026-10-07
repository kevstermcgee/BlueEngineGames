# Deadfall upgrade verification — 2026-10-07

## What changed

The native game retains its existing visual style and server-authoritative 60 Hz simulation. Slagworks gains spawn cover and new pickups. Switchyard adds compact freight-yard lanes and loading overlooks; Stormbreak adds coastal relay towers, generators and elevated approaches. All three have collision-aware paths to every spawn, weapon, flag base and bomb site.

Team Deathmatch and Free for All support a two-human duel with opposing teams where applicable, no automatic bots and a two-seat room cap. Capture the Flag requires returning the enemy flag while the home flag is present; dropped flags return after twenty seconds. Search and Destroy alternates attacking sides, has no respawns during a round, and requires continuous three-second planting or five-second defusing. A planted bomb continues after the attacker dies. The first configured capture/round target wins.

Hornet is a burst sidearm, Ranger a scoped cycling marksman rifle, and Breach-8 a slug shotgun. Existing weapon IDs stay fixed; the new IDs are 34–36. Rifleman, Scout, Recon and Breacher share the existing skeleton and hitboxes, with four skin options per team. Faster movement, higher vaultable jumps and four-second deathmatch respawns reduce downtime.

## Network and engine evidence

The engine upgrade fixes a reproduced mismatch between the encoder's packet limit and the active 1100-byte transport budget. Deadfall opts into independent sequenced combat events, bounded retransmission and reordering, original event timestamps, session tokens and match epochs. Tests include burst traffic, 30% loss, delayed/reordered traffic, oversized events and repeated matches. Existing games retain their original wire behavior.

The game snapshot prioritizes the essential roster, inventory and objectives before nearby transient effects. Real hub integration exercises all eight stable settings, all four modes, requested maps, replicated cosmetics, one-player waiting and refusal of a third duel client. Old discovery clients receive an update notice; game protocol 2 requires both friends and the live server to update.

Map navigation is prepared before the server accepts players. This removes a measured 20–26 ms first-match hitch from the steady server tick. Bot navigation fixtures complete flag captures and bomb planting on every map; competitive bot combat has separate multi-seed soaks. An isolated navigation fixture does not promise that two competing bot teams avoid stalemates.

## Verification receipts

Final native game checks, engine Linux/Windows checks, package smoke, visual review and publication receipts are recorded below when completed.

## Practical limits

Loopback and simulated packet-loss tests establish behavior under their fixtures; they do not establish interstate routing, Wi-Fi quality or a particular player's latency. The shared public hub currently uses UDP without TLS. Reliable combat-event retention is bounded to 4096 events, 1 MiB and ten seconds; explicit gaps allow recovery if that window is exhausted. Software-rendered review captures establish visibility and layout, not hardware frame rate or subjective weapon feel. Playing a full match with the cousin remains useful feedback after release.

## Reproduce

Run `cargo fmt --all --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked --no-default-features`. Set `BE2_HUB` to the matching engine's built `be2-hub` so actual room tests are mandatory. Run the game's `scripts/check.py` and `scripts/ship.py ship` for native identity/package checks. The catalog CI runs game checks on native Linux and Windows; Windows publication builds the installer.

Engine feedback lives in `docs/ENGINE_FEEDBACK.md` and the engine's `docs/feedback/2026-10-07-deadfall-networking.md`, with regression suites and retrievable learning records L-086 through L-088.
