# Deadfall: guide for the next person (or agent) to touch it

Deadfall is a six-versus-six team deathmatch shooter on BlueEngine, built with the engine's `netplay` kit. Read
`DESIGN.md` (the brief and the weapon roster) and `README.md` (how it plays) first.

## Layout
* `src/lib.rs`: the game as a plain library, no window. `weapons` (the 33-weapon armoury), `hands` (the per-player weapon state
  machine: fire, reload, ADS, grenades, melee; shared by the server and the predicting client), `sim` (the authoritative
  match), `bots` + `nav` (computer players on a 3D walkable graph), `netgame` (wire layouts, the `NetGame` impl, the client
  view with prediction and the killcam history), `slagworks` (the map as boxes, spawns, loot, decor), `stats` and `prefs`.
* `src/client/`: the window (feature `client`): `app` (screens and the frame loop), `render`, `level_view`, `character`, `arms`,
  `weapon_models`, `overlay` (HUD), `ui` (menus, text fields with paste), `online` (Play Online: room list, Create Room, join retry
  rules; the state machine and pure rules, no window), `controls`, `sound` + `audio` (all sound is synthesised).
* Binaries: `deadfall` (the game), `deadfall-server` (headless dedicated server), `preview*` and `audio_dump` (look at one thing).

## Rules of the house
* Server authority: clients send intentions (`input::Input`); the server decides hits with lag compensation (`Input::seen_tick`).
  A client predicts its own body and hands by running the same `sim::step_body` and `Hands::tick`.
* A press that must not be lost is a **counter** in `Input`, never an edge bit.
* Changing a weapon number, the map, or any wire layout changes `netgame::fingerprint()`, so old clients are refused. Weapon wire
  ids are positions in `weapons::WEAPONS`: append, never reorder.
* No blood, no music, no voices. Ambience stems and one-shots live in `client/audio.rs`.
* Gameplay keys are read straight from macroquad in `controls.rs`: the engine's `ClientInput` tracks only a fixed list of keys on Windows.

## Looking at it without a screen
`xvfb-run` plus `deadfall --solo --capture DIR --frames 200,600 --mute` saves screenshots; `--script "ads:100-300,fire:150-200,slot1@10,accept@400"`
plays the local player; `DEADFALL_AUTOPILOT=1` lets a bot drive you; `DEADFALL_GIVE=awm,frag` starts you with those weapons;
`DEADFALL_DATA=DIR` redirects stats and preferences. Online screens: `--hub 127.0.0.1:PORT --screen online` against a loopback
`deadfall-hub` (see `src/bin/hub.rs`); `DEADFALL_FAKE_BUILD=N` pretends to be another build to see the version-mismatch state.

## Tests
`cargo test` runs the armoury checks, combat rules, 12-bot soaks, an online match on a lossy simulated network and a 20-second
match over real UDP sockets. They need no window.
