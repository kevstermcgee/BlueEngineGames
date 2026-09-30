# Spooky Kart: design and decisions

A Halloween kart racer on BlueEngine. One map, eight drivers, each with a unique kart and perk. Hosted
multiplayer on the Debian mini PC; clients install from BlueEngineGames. It exists to make the engine
better: what it teaches us about networking, mechanics and character design goes back into BlueEngine.

## Decisions (agreed with Kevin, 2026-09-29)
- **Name**: Spooky Kart (with a K).
- **Networking**: game-owned authoritative server on the engine's transport/session/QUIC pieces, with
  client prediction for the local kart and interpolation for the rest. Extract what proves reusable into an
  engine kit afterwards. The engine's own `DedicatedServer` is built around walking characters, so it is not used.
- **Bots**: deterministic AI drivers fill empty grid slots, give solo play, and are the load-test clients.
- **Items**: none in v1. Each character has a stat spread plus one signature perk.
- **Repo**: its own repository, published into BlueEngineGames as an independently maintained game (a
  `preserve` path, like Skyhook Sprint).
- **Scale**: at most 8 racers, matching the engine server's session cap. One map: Haunted Hollow.

## Architecture
- `src/lib.rs` and modules: the pure simulation. Fixed 60 Hz `Sim::step(&Inputs)`. No window, no wall clock.
  Bots run inside the simulation (`bot::drive`), so they are deterministic and part of saves.
- `src/main.rs`: window, camera, rendering, sound, menus. Never decides gameplay.
- f32 trig may differ across platforms, so multiplayer does not rely on lockstep: the server is
  authoritative, clients predict and reconcile.

## Drivers
| Character | Kart feel | Perk |
|---|---|---|
| Vampire | fast acceleration, medium weight | Bat Boost (passive): drift charges faster, stronger boost |
| Frankenstein's Monster | heavy, slow to accelerate, high top speed | Ram (passive): bumps launch lighter karts |
| Mummy | great handling | Bandage Trail (active): strip that slows karts behind |
| Ghost | lightest, floaty | Phase (active): passes through karts, hazards and off-road for a moment |
| Scarecrow | balanced | Crow Swarm (active): slows the nearest kart ahead |
| Zombie | slow but sturdy | Undead (passive): little off-road penalty, short stuns |
| Clown | highest handling, lower top speed | Honk (active): knockback burst around the kart |
| Skeleton | light, fast acceleration | Rattle (active): drops bone hazards, immune to hazards |

Numbers live in one table (`character.rs`) so balance changes are data edits, and telemetry per race
(`RaceReport`) says whether they work: win rate, drift time, wall hits, collisions, perk uses per character.

## Milestones
1. Pure simulation: kart physics, track, race, eight characters and perks, bots, telemetry, tests.
2. Window: track and karts rendered, chase camera, HUD, character select, offline race with bots.
3. Multiplayer: server on the Debian box, prediction/reconciliation, lobby, rematch.
4. Metrics: server writes per-race JSONL; bot-driven load test recorded through BlueEngine's `tools/perf.py`.
5. Ship: identity and icon, `scripts/blue ship`, publish to BlueEngineGames.
