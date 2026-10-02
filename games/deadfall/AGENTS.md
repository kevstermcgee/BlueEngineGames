# Deadfall: guide for the next person (or agent) to touch it

Deadfall is a six-versus-six team deathmatch shooter on BlueEngine, built with the engine's `netplay` kit. Read
`DESIGN.md` (the brief and the weapon roster) and `README.md` (how it plays) first.

## Layout
* `src/lib.rs`: the game as a plain library, no window. `weapons` (the 33-weapon armoury), `hands` (the per-player weapon state
  machine: fire, reload, ADS, grenades, melee; shared by the server and the predicting client), `sim` (the authoritative
  match), `bots` + `nav` (computer players on a 3D walkable graph), `netgame` (wire layouts, the `NetGame` impl, the client
  view with prediction and the killcam history), `slagworks` (the map as boxes, spawns, loot, decor), `stats` and `prefs`.
* `src/client/`: the window (feature `client`): `app` (screens and the frame loop), `render`, `level_view`, `character`, `arms`,
  `weapon_models`, `overlay` (HUD), `ui` (menus, text fields with paste), `online` (Play Online: Deadfall's side of the engine's hub client,
  `netplay::hub::client`: build id and `DEADFALL_FAKE_BUILD`, strings, `visible_rows`; the state machine and rules are the engine's), `controls`, `sound` + `audio` (all sound is synthesised).
* Binaries: `deadfall` (the game), `deadfall-server` (headless dedicated server: `netplay::cli::serve::<DeadfallGame>`, nothing hand-rolled),
  `preview*` and `audio_dump` (look at one thing). There is no Deadfall hub any more: rooms are listed and started by the engine's
  shared `be2-hub` (ADR 0037), which runs `deadfall-server` once per room (see `deploy/README.md`).
* Room settings are `netgame::SETTINGS` (ids 1 `bots`, 2 `kills`, 3 `skill`, 4 `minutes`): ids are on the hub's wire and in its registry, so never
  renumber or reuse one, and keep the names `bots` and `kills`, which the hub's legacy adapter maps the old clients' Create request to.
  `NetGame::configure` turns them into `sim::Settings` (the process-global; one process per room).

## Rules of the house
* Server authority: clients send intentions (`input::Input`); the server decides hits with lag compensation (`Input::seen_tick`).
  A client predicts its own body and hands by running the same `sim::step_body` and `Hands::tick`.
* A press that must not be lost is a **counter** in `Input`, never an edge bit.
* Changing a weapon number, the map, or any wire layout changes `netgame::fingerprint()`, so old clients are refused: it is pinned in
  `netgame::tests::the_join_fingerprint_is_pinned` (and again by `tests/server_cli.rs`), because the shipped Windows clients compare the hub's raw
  fingerprint with their own and the hub's legacy adapter reports it unchanged. Weapon wire
  ids are positions in `weapons::WEAPONS`: append, never reorder.
* No blood, no music, no voices. Ambience stems and one-shots live in `client/audio.rs`.
* Shadows are presentation only (never touch `sim`, `netgame` or the map for them: `netgame::fingerprint()` must not change). The
  setting is `Prefs::shadows` (Off / Simple / Full), the `--shadows` flag overrides it for one run, and `client/render.rs` owns the
  engine's `kit::Shadows`: Simple draws a blob under each soldier and loot item (ground from the level's block tops), Full adds
  one shadow pass of the same batches. `LevelScene::solid` and `decor` cast; `LevelScene::ground` (the slabs, ground cover and far
  scenery) only receives. Casting meshes are sorted into 20 m cells (`level_view::Grid`) so the pass can skip those outside the box.
* Gameplay keys are read straight from macroquad in `controls.rs`: the engine's `ClientInput` tracks only a fixed list of keys on Windows.

## Looking at it without a screen
`xvfb-run` plus `deadfall --solo --capture DIR --frames 200,600 --mute` saves screenshots; `--script "ads:100-300,fire:150-200,slot1@10,accept@400"`
plays the local player; `DEADFALL_AUTOPILOT=1` lets a bot drive you; `--shadows off|simple|full` picks the shadow tier (the capture path honours it); `DEADFALL_GIVE=awm,frag` starts you with those weapons;
`DEADFALL_DATA=DIR` redirects stats and preferences. Online screens: `--hub 127.0.0.1:PORT --screen online` against a loopback
`be2-hub` (build it in the engine: `CARGO_TARGET_DIR=~/.cache/be-engine-target cargo build --profile fast --bin be2-hub`; its registry
is `deploy/hub/hub.conf.example` with `listen = 127.0.0.1:PORT`, a pool in 43000-44999 and `server =` the built `deadfall-server`);
`DEADFALL_FAKE_BUILD=N` pretends to be another build to see the version-mismatch state. The window renders about one frame a second under
`xvfb` without a GPU, so keep `--frames` small (90 is enough for the list; `--script "down@50,down@54,accept@58"` opens Create Room by frame 100).
The default hub is the engine's chain (`--hub`, `server.txt`, last used, `blue-engine.duckdns.org:4100`); `deadfall-kevin.duckdns.org` stays
valid for already-shipped builds.

`preview_level --what walk --at x,y,z --angles 0 --out DIR --shadows full --men x:z:yaw,x:z:yaw` draws a fixed spot through the game's
own renderer, with soldiers standing where you say and the average frame time printed: the way to compare the three shadow tiers
without a moving match.

## Tests
`cargo test` runs the armoury checks, combat rules, 12-bot soaks, an online match on a lossy simulated network and a 20-second
match over real UDP sockets. They need no window. `tests/engine_hub.rs` runs the engine's `be2-hub` binary with the real `deadfall-server`
on loopback (BEHB, the old DFHB v1 with a hand-written copy of the shipped codec, and two real clients joining); it skips with a message
when `be2-hub` is not built (set `BE2_HUB=/path/to/be2-hub` or build it into `~/.cache/be-engine-target`).
