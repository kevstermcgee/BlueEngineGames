# Deadfall

Fast multiplayer combat for two to twelve players on BlueEngine, with optional bots. **Team Deathmatch**, **Capture the Flag**, **Search and Destroy**, and **Free for All** all support a two-player duel. Teams are **Ironclad** (army green and tan) and **Nightwatch** (navy and black).

Three maps offer different fights: **Slagworks**, the expanded steelworks with roof routes, offices and new weapon placements; **Switchyard**, a compact freight yard with container flanks, an open central shed and loading overlooks; and **Stormbreak**, a coastal relay station with rooftop stairs and long outside lanes.

Everything (models, sounds, map) is generated in code; the game is one executable.

## Playing

* **Play Online**: the easy way to play with friends. Both of you click **Play Online**: you see a list of rooms (the *Public*
  room first, then rooms people made, with player counts and *Lobby* / *In match*). Click a room to join it, or **Create Room**,
  name it, pick a map and mode, leave **1v1 duel** enabled, and your friend picks it from the same list. The host cannot start until both players join and press Ready. **Copy invite** in the lobby copies the room name and hub address. Nobody types an address or opens a router port. If a match is already
  running the game says so and puts you in the next round. Both players need the same game version (the build id shows in the
  corner of the main menu).
* **Solo**: you and bots against bots. Pick your team, map, mode, match target and bot skill.
* **Host on this PC (advanced)**: start a server on your computer for friends. By default **only the people who join play**; switch on *Bots fill the
  teams* to fill both sides to six. Choose how the match ends: first team to N kills, or a time limit.
* **Join by address (advanced)**: type or paste (Ctrl+V) the host's address (`address:port`, the port defaults to 4100) and, if the host set one, the password. Pick your team
  in the lobby, press **Ready**; human-only matches require at least two players and start when everyone is ready.
* **Settings**: name, mouse and stick sensitivity, invert look, volume, fullscreen and **Shadows**: *Simple* (the default: a soft
  contact shadow under every soldier and dropped weapon, nearly free), *Full* (real cast shadows from buildings, containers,
  trees, crates and soldiers; costs frame rate, most on slow graphics) or *Off*. Pick **Rifleman**, **Scout**, **Recon**, or **Breacher**, and one of four skin tones. Appearance is replicated by the server and your first-person hands match it. Settings are remembered; `--shadows off|simple|full` picks
  one for a single run.
* **Stats**: your lifetime kills, deaths, headshots, accuracy, matches, time played and more are kept on this computer.

### Playing with friends over the internet

Use **Play Online**: the rooms live on the shared BlueEngine hub (`be2-hub`, one DNS name for every BlueEngine game; see
`deploy/README.md` and the engine's `docs/adr/0037-shared-multi-game-hub.md`). The game looks for the hub in this order: `--hub HOST:PORT`,
a `server.txt` next to the game (first line `host` or `host:port`), the hub you last chose with `--hub` and joined a room on, then the
built-in `blue-engine.duckdns.org:4100`. Older builds can discover the hub and receive an update notice: they ask `deadfall-kevin.duckdns.org:4100`, which
the same hub answers in the old discovery protocol. This release changes the game protocol: everyone must install the new version. Hosting on your own PC is the advanced route: the host needs UDP port **4100** reachable: forward it on the router to the host's computer (or put everyone on a VPN such as
Tailscale or ZeroTier and use the VPN address). Friends join with the host's public address. The game's network layer is
raw UDP with session authentication and an optional join key; hub rooms use the development transport without TLS. Combat events use a bounded acknowledged stream, independently of movement snapshots. Name lookup runs off the render thread, and failed joins offer a retry. A dedicated server also exists:
`deadfall-server --listen 0.0.0.0:4100 [--kills 40 | --minutes 10] [--bots] [--skill 0|1|2] [--map 0|1|2] [--mode 0|1|2|3] [--duel] [--objective 3]` (the engine's shared server flags plus
`--set ID=VALUE`, `--status-lines`, `--exit-on-stdin-eof`, `--info`; `--minutes` above 0 wins over `--kills`; out-of-range values are
refused rather than clamped).

## Controls

| | Keyboard and mouse | Controller |
|---|---|---|
| Move / look | WASD / mouse | left stick / right stick |
| Fire / aim down sights | left mouse / right mouse | right trigger / left trigger |
| Jump / crouch | Space / Ctrl | A / B (hold) |
| Reload / use (pick up) | R / E | X / right bumper |
| Plant / defuse | hold E at a bomb site / bomb | hold right bumper |
| Weapons | 1 primary, 2 secondary, 3 knife, 4 grenade, mouse wheel | D-pad up / right / down, left bumper, Y |
| Quick knife / drop weapon | Q / G | right stick click / D-pad left |
| Walk quietly | Shift | left stick click |
| Scoreboard / menu | Tab / Esc | Back / Start |

You carry two firearms (a primary and a secondary), one melee weapon and up to two grenades. Everyone starts with the K-9 pistol and
the combat knife; the other 34 weapons lie around the map (walk over one to take it when the slot is empty, press **E** to swap).
Every weapon has its own ammunition. After you die a four-second killcam shows you how, from your killer's eyes, then you return.

## Modes

* **Team Deathmatch**: first team to the kill target, or most kills when time expires. Two players are assigned opposing teams even if both choose the same one.
* **Capture the Flag**: touch the enemy flag, carry it home, and capture while your own flag is home. Touch your dropped flag to return it; untouched flags return after 20 seconds. Default: first to three captures.
* **Search and Destroy**: attackers carry a bomb to either marked site. Hold E/right bumper for three seconds to plant, or five seconds to defuse as a defender. No respawns within a round. Attack/defence alternates each round; a planted bomb has a 35-second fuse and survives attacker elimination. Default: first to three rounds. After the killcam, eliminated players spectate until the next round.
* **Free for All**: everyone is an opponent, with individual kills and a winner shown by name. Enable 1v1 for a private two-player match.

1v1 disables bots and limits the room to two humans. Leaving a live duel awards a forfeit. Normal matches can fill vacant slots with bots. Movement is faster, jump clearance is higher, and deathmatch respawns are shorter.

## Weapons

Thirty-six, from pistols to a rocket launcher, with stats modelled on their real counterparts; see
[docs/ARMOURY.md](docs/ARMOURY.md). New additions are the **Hornet Burst** pistol, **Ranger Lever Rifle**, and **Breach-8 Slug** shotgun; all are available as map pickups.

## Building

`cargo build --release --bin deadfall` (needs the BlueEngine checkout next to this repository, see `Cargo.toml`).
`cargo test` runs the rules, the bots and an online match on a simulated lossy network without opening a window.
