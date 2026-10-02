# Deadfall online hub: putting it on the internet

What this sets up on the always-on box: one small program, `deadfall-hub`, listens on UDP 4100. Players click Play Online,
the game asks the hub for the list of rooms, and they pick one by name or make one. The hub starts one `deadfall-server`
per room (the permanent **Public** room on 4101, player rooms on 4102-4107) and closes player rooms that stay empty for
two minutes. Nobody types an IP or a code. Nothing here is installed by the repository: these are the steps for a human.

Files in this directory:

| File | What it is |
| --- | --- |
| `deadfall-hub.service` | systemd user service running the hub (which starts the room servers) |
| `deadfall-portmap.service`, `.timer` | every 30 minutes, renews the router's UPnP mappings for UDP 4100-4107 |
| `deadfall-ddns.service`, `.timer`, `deadfall-ddns.sh` | every 5 minutes, points the DuckDNS name at the home IP |
| `update.sh` | builds the hub and server from `origin/main` and restarts the hub when `games/deadfall` changed |

## 0. Before you start: the port clash with Spooky Kart

Spooky Kart's server also defaults to **UDP 4100**, and two programs cannot listen on one UDP port on one machine. If
Spooky Kart is running on this box, either stop it or move Deadfall to another range, for example 4200-4207:

* in `deadfall-hub.service` change `--listen 0.0.0.0:4100 --base-port 4100` to `--listen 0.0.0.0:4200 --base-port 4200`;
* in `deadfall-portmap.service` change `seq 4100 4107` to `seq 4200 4207`;
* tell players the port: put `deadfall-kevin.duckdns.org:4200` in `server.txt` next to the game (see step 7), or change
  `DEFAULT_HUB` in `src/hub_client.rs` before building the client.

The hub refuses to start with a clear message if its port is taken. Check what is listening with
`ss -lunp | grep -E ':41[0-9][0-9]'`.

## 1. DuckDNS name (free, once)

1. Sign in at <https://www.duckdns.org> (a Google, GitHub or similar account).
2. Add a sub-domain, e.g. `deadfall-kevin` (the name becomes `deadfall-kevin.duckdns.org`). If you pick another name, change
   `DEFAULT_HUB` in `src/hub_client.rs` (the one place the name is written) before the next game release, or use
   `server.txt`.
3. Copy your **token** from the top of the DuckDNS page. It is a secret: it can repoint your name.

## 2. Put the token in a file that is never in git

```sh
mkdir -p ~/.config/deadfall
cat > ~/.config/deadfall/duckdns.env <<'EOT'
DOMAIN=deadfall-kevin
TOKEN=paste-your-token-here
EOT
chmod 600 ~/.config/deadfall/duckdns.env
```

`DOMAIN` is the name **without** `.duckdns.org`. The update script passes the token to curl on stdin, so it never shows
up in `ps`, in the unit file or in the journal.

## 3. Build and install the programs

With this directory merged to `main` and `~/BlueEngineGames` pulled (the script builds in a detached worktree,
`~/BlueEngineGames-deploy`, next to `~/BlueEngine` so the engine path dependency resolves):

```sh
cd ~/BlueEngineGames && git pull
bash games/deadfall/deploy/update.sh --force
```

This builds `deadfall-hub` and `deadfall-server` in release mode (no graphics libraries needed) and installs them, the
DuckDNS script and `blue_portmap.py` into `~/deadfall/`. It also creates `~/.local/share/deadfall/`, where match reports go.

## 4. Install and start the services

```sh
mkdir -p ~/.config/systemd/user
cp games/deadfall/deploy/deadfall-hub.service games/deadfall/deploy/deadfall-portmap.* \
   games/deadfall/deploy/deadfall-ddns.service games/deadfall/deploy/deadfall-ddns.timer ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now deadfall-hub.service deadfall-portmap.timer deadfall-ddns.timer
loginctl enable-linger "$USER"      # keep user services running when nobody is logged in, and start them at boot
```

`enable-linger` is what makes it "always on". Run the two timed jobs once right away instead of waiting:

```sh
systemctl --user start deadfall-ddns.service deadfall-portmap.service
```

If `systemctl --user status deadfall-hub` shows `status=226/NAMESPACE`, the sandbox lines are not allowed for user
services on this kernel: comment out `ProtectSystem`, `ReadWritePaths` and `PrivateTmp` in the unit and reload.

## 5. Check that it works

```sh
systemctl --user status deadfall-hub deadfall-portmap.timer deadfall-ddns.timer
journalctl --user -u deadfall-hub -n 30            # "Deadfall hub on 0.0.0.0:4100 ... Public room is up on port 4101"
ss -lunp | grep -E ':41(0[0-9])\b'                 # 4100 (hub) and 4101 (Public) listening; each player room adds one
python3 ~/deadfall/blue_portmap.py status --port 4100   # router mapping and the public IPv4 (repeat for 4101..4107)
journalctl --user -u deadfall-ddns -n 5            # "DuckDNS: deadfall-kevin.duckdns.org updated"
getent hosts deadfall-kevin.duckdns.org                  # should print your public IP
```

Then, from a different network (a phone hotspot, or your cousin): start the game, Play Online. The Public room should be
listed. UPnP must be enabled on the router; if `blue_portmap.py` cannot map the ports, forward UDP 4100-4107 to this
machine by hand in the router's page.

## 6. Updating

```sh
bash ~/BlueEngineGames/games/deadfall/deploy/update.sh        # nothing happens if games/deadfall did not change
```

It fetches `origin/main`, compares the `games/deadfall` tree with the last install, and only then builds, installs and
restarts `deadfall-hub` (which ends the rooms that are running, so do it when nobody is playing). `--force` rebuilds
anyway, which you need after changing the engine in `~/BlueEngine`. Players need the matching game version: the hub
sends its build id with the room list, and an older client is told to update before it tries to join.

Rooms, players and limits are changed in the `ExecStart` line of `deadfall-hub.service` (`--max-rooms`, `--public-name`,
`--report-dir`); run `~/deadfall/deadfall-hub --help` for all flags. If you raise `--max-rooms`, widen the port range in
`deadfall-portmap.service` to `base-port` .. `base-port + 1 + max-rooms`.

## 7. Pointing a game at a different hub (testing, or a different port)

The game uses `deadfall-kevin.duckdns.org:4100` unless a file called `server.txt` sits next to the game executable. Its first
line is the hub, as `host` or `host:port` (a LAN test: `192.168.1.20`). Delete the file to go back to the default.

## Stopping and removing

```sh
systemctl --user disable --now deadfall-hub.service deadfall-portmap.timer deadfall-ddns.timer
python3 ~/deadfall/blue_portmap.py remove --port 4100      # and 4101..4107, if you want the router mappings gone now
```

Stopping the hub stops every room it started. Stopped or crashed, the servers never outlive the hub: they quit when it goes.
