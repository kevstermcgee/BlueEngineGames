# Deadfall

Team deathmatch for up to twelve players, built on BlueEngine. Six against six, online or against bots.
Two teams, **Ironclad** (army green and tan) and **Nightwatch** (navy and black), fight over **Slagworks**, an
abandoned steelworks: a smelter hall, a two-floor control office, a warehouse, a container yard, a pipe alley, a rail
yard, a loading dock and a boiler house, overgrown with trees and weeds.

Everything (models, sounds, map) is generated in code; the game is one executable.

## Playing

* **Play Online**: the easy way to play with friends. Both of you click **Play Online**: you see a list of rooms (the *Public*
  room first, then rooms people made, with player counts and *Lobby* / *In match*). Click a room to join it, or **Create Room**,
  name it, and your friend picks it from the same list. Nobody types an address or opens a router port. If a match is already
  running the game says so and puts you in the next round. Both players need the same game version (the build id shows in the
  corner of the main menu).
* **Solo**: you and bots against bots. Pick your team, how the match ends and the bot skill.
* **Host on this PC (advanced)**: start a server on your computer for friends. By default **only the people who join play**; switch on *Bots fill the
  teams* to fill both sides to six. Choose how the match ends: first team to N kills, or a time limit.
* **Join by address (advanced)**: type or paste (Ctrl+V) the host's address (`address:port`, the port defaults to 4100) and, if the host set one, the password. Pick your team
  in the lobby, press **Ready**; the match starts when everyone is ready.
* **Settings**: name, mouse and stick sensitivity, invert look, volume, fullscreen and **Shadows**: *Simple* (the default: a soft
  contact shadow under every soldier and dropped weapon, nearly free), *Full* (real cast shadows from buildings, containers,
  trees, crates and soldiers; costs frame rate, most on slow graphics) or *Off*. It is remembered; `--shadows off|simple|full` picks
  one for a single run.
* **Stats**: your lifetime kills, deaths, headshots, accuracy, matches, time played and more are kept on this computer.

### Playing with friends over the internet

Use **Play Online** (the rooms live on a hub that runs `deadfall-hub`; see `deploy/README.md`). `--hub HOST:PORT` or a `server.txt`
next to the game picks another hub. Hosting on your own PC is the advanced route: the host needs UDP port **4100** reachable: forward it on the router to the host's computer (or put everyone on a VPN such as
Tailscale or ZeroTier and use the VPN address). Friends join with the host's public address. The game's network layer is
raw UDP with an optional password; it is meant for friends, not the open internet. A dedicated server also exists:
`deadfall-server --listen 0.0.0.0:4100 [--kills 40 | --minutes 10] [--bots] [--skill 0|1|2]`.

## Controls

| | Keyboard and mouse | Controller |
|---|---|---|
| Move / look | WASD / mouse | left stick / right stick |
| Fire / aim down sights | left mouse / right mouse | right trigger / left trigger |
| Jump / crouch | Space / Ctrl | A / B (hold) |
| Reload / use (pick up) | R / E | X / right bumper |
| Weapons | 1 primary, 2 secondary, 3 knife, 4 grenade, mouse wheel | D-pad up / right / down, left bumper, Y |
| Quick knife / drop weapon | Q / G | right stick click / D-pad left |
| Walk quietly | Shift | left stick click |
| Scoreboard / menu | Tab / Esc | Back / Start |

You carry two firearms (a primary and a secondary), one melee weapon and up to two grenades. Everyone starts with the K-9 pistol and
the combat knife; the other 31 weapons lie around the map (walk over one to take it when the slot is empty, press **E** to swap).
Every weapon has its own ammunition. After you die an eight-second killcam shows you how, from your killer's eyes, then you return.

## Weapons

Thirty-three, from pistols to a rocket launcher, with stats modelled on their real counterparts; see
[docs/ARMOURY.md](docs/ARMOURY.md).

## Building

`cargo build --release --bin deadfall` (needs the BlueEngine checkout next to this repository, see `Cargo.toml`).
`cargo test` runs the rules, the bots and an online match on a simulated lossy network without opening a window.
