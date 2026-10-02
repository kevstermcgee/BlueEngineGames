# Deadfall online: hosted by the shared BlueEngine hub

Deadfall no longer has a hub of its own. Play Online talks to the engine's shared hub, `be2-hub`, which lists and creates rooms
for every BlueEngine game on one UDP port and one DuckDNS name (`blue-engine.duckdns.org:4100`). The files that used to live here
(systemd units, port mapping, DuckDNS updater, `update.sh`) are now the engine's:

* how to set the always-on box up, step by step: `~/BlueEngine/deploy/hub/README.md` (units, `hub.conf`, `sources.conf`, `update.sh`);
* why it is built that way: `~/BlueEngine/docs/adr/0037-shared-multi-game-hub.md`;
* the registry entry Deadfall needs, from `deploy/hub/hub.conf.example`:

```ini
[game deadfall]
server = /home/kevin/blueengine/deadfall-server
public = on
public_set = bots=1
user_set = kills=40
client_settings = bots,kills
max_rooms = 4
auto_start = 30
```

* the build line for `sources.conf` (the server needs no window):
  `deadfall  ~/BlueEngineGames/games/deadfall  deadfall-server  --no-default-features`.

What the hub learns from `deadfall-server --info`: `game=deadfall`, the raw join fingerprint, 12 seats and four settings with
stable ids: `1 bots` (on/off), `2 kills` (1-500, default 40), `3 skill` (0-2, default 1) and `4 minutes` (0-60; above 0 the match
ends on the clock instead of the kill target). `bots` and `kills` are the names the hub's legacy adapter maps the shipped
clients' old Create request to.

**Already shipped builds** speak the old `DFHB` protocol to `deadfall-kevin.duckdns.org:4100`. The engine hub answers it on the
same port (`legacy = serve`), so keep that DuckDNS name pointing at the box next to `blue-engine`. New builds default to
`blue-engine.duckdns.org:4100`; `--hub HOST:PORT` or a `server.txt` next to the game overrides it.
