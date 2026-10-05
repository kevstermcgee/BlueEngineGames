# Hosting a Prop Hunt server

One small process runs the lobby, the hide/seek rounds and the results screen for up to eight players
(seven hiders and one seeker); there is no bot AI to fill empty seats or replace someone who drops out
(`NetGame::release` just marks a departed hider tagged, or ends the round early if the seeker leaves — see
`src/netgame.rs`). On the reference machine (an Intel N97 mini PC, the same box Spooky Kart runs on) expect
similar load to Spooky Kart: well under 2% of one core and a few MB of memory with two players; see
`docs/perf` in BlueEngine (`python tools/perf.py report`).

## Build and run

```sh
cargo build --release --no-default-features --bin prop-hunt-server
install -D target/release/prop-hunt-server ~/.local/bin/prop-hunt-server
prop-hunt-server --listen 0.0.0.0:4101 --transport development          # LAN or testing: raw UDP
```

Players connect with `prop-hunt --connect SERVER_IP:4101` (add `--transport production` and `--join-key
KEY` for a production server). Use UDP port **4101** (Spooky Kart already uses 4100 on this box; pick a
different port per game so both can run at once).

## Production (the internet)

`--transport production` is QUIC over TLS 1.3, the engine's own secure transport, with a pinned
certificate. Provision the server key and certificate outside the repository:

```sh
export BLUE_TLS_KEY_FILE=~/.config/blueengine/server-key.der     # PKCS#8 DER private key, mode 600
export BLUE_TLS_CERT_FILE=~/.config/blueengine/server-cert.der   # the matching public certificate
```

These are the same environment variables and the same certificate Spooky Kart uses; one certificate
pair can serve both games on this box. To make a certificate of your own (the name must be
`feta.local`). It must **not** be flagged as a CA: the TLS library refuses a CA certificate as a
server's identity, and the failure only shows as "Could not reach the server".

```sh
openssl ecparam -genkey -name prime256v1 -noout -out key.pem
openssl pkcs8 -topk8 -nocrypt -in key.pem -outform DER -out server-key.der
openssl req -new -x509 -key key.pem -subj "/CN=feta.local" -days 365 \
  -addext "subjectAltName=DNS:feta.local" -addext "basicConstraints=critical,CA:FALSE" \
  -addext "keyUsage=critical,digitalSignature" -addext "extendedKeyUsage=serverAuth" \
  -outform DER -out server-cert.der
chmod 600 server-key.der
```

Give players the same public certificate (`BLUE_TLS_CERT_FILE`) or use the engine's bundled one for both
sides. The certificate name must be `feta.local`; connect by IP or DNS name as usual. Also set a join key
so strangers who find the port cannot start a round: `PROP_HUNT_JOIN_KEY=...` (or `--join-key`); it
travels inside the encrypted channel. Development UDP is unencrypted; keep it on a trusted network.

## As a service

`deploy/prop-hunt-server.service` is a hardened systemd user unit, the same shape as
`spooky-kart-server.service` so both games can run side by side on the same box.

```sh
mkdir -p ~/.config/prop-hunt ~/.local/share/prop-hunt
printf 'PROP_HUNT_JOIN_KEY=%s\nBLUE_TLS_KEY_FILE=%s\n' "LONG_RANDOM_SECRET" "$HOME/.config/blueengine/server-key.der" > ~/.config/prop-hunt/server.env
chmod 600 ~/.config/prop-hunt/server.env
install -D deploy/prop-hunt-server.service ~/.config/systemd/user/prop-hunt-server.service
systemctl --user daemon-reload && systemctl --user enable --now prop-hunt-server
loginctl enable-linger "$USER"        # keep it running when nobody is logged in
```

Open UDP 4101 on the router and firewall only when you want outside players. (4100 is Spooky Kart's;
keep the two apart.)

## What it records

The netplay kit (`vesper3d::viewer::netplay::server`) appends one line per finished round to
`matches.jsonl` in `--report-dir` generically for any `NetGame`, the same mechanism Spooky Kart uses:
`NetGame::report()` (Prop Hunt's outcome, round length and per-participant tagged/seeker-preference data,
see `Sim::report` in `src/sim.rs`) plus each player's network quality (round-trip time, bytes, repeated or
skipped inputs) and the server's own tick times. It is plain JSON lines, so
`python -c "import json,sys; [print(json.loads(l)['report']) for l in open(sys.argv[1])]"` or `jq` reads it
directly; Prop Hunt does not yet ship a dedicated analysis tool the way Spooky Kart's
`tools/analyze.py` reads kart-specific per-character stats (there is no per-character balance question
here) — add one if per-role (hider vs. seeker) win-rate tuning turns out to need it.
