# Hosting a Slapstick server

One small process runs the lobby, the match and the results screen for exactly two players; a departed
player's paddle is handed to the bot AI rather than ending the match (`NetGame::release` just flips
`paddle.ai`, see `src/netgame.rs`). On the reference machine (an Intel N97 mini PC, the same box Spooky
Kart and Prop Hunt run on) expect similar load to those two games: well under 2% of one core and a few
MB of memory with two players; see `docs/perf` in BlueEngine (`python tools/perf.py report`).

## Build and run

```sh
cargo build --release --no-default-features --bin slapstick-server
install -D target/release/slapstick-server ~/.local/bin/slapstick-server
slapstick-server --listen 0.0.0.0:4102 --transport development          # LAN or testing: raw UDP
```

Players connect with `slapstick --connect SERVER_IP:4102` (add `--transport production` and `--join-key
KEY` for a production server). Use UDP port **4102** (Spooky Kart uses 4100 and Prop Hunt uses 4101 on
this box; pick a different port per game so all three can run at once).

## Production (the internet)

`--transport production` is QUIC over TLS 1.3, the engine's own secure transport, with a pinned
certificate. Provision the server key and certificate outside the repository:

```sh
export BLUE_TLS_KEY_FILE=~/.config/blueengine/server-key.der     # PKCS#8 DER private key, mode 600
export BLUE_TLS_CERT_FILE=~/.config/blueengine/server-cert.der   # the matching public certificate
```

These are the same environment variables and the same certificate Spooky Kart and Prop Hunt use; one
certificate pair can serve all three games on this box. To make a certificate of your own (the name must
be `feta.local`). It must **not** be flagged as a CA: the TLS library refuses a CA certificate as a
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
so strangers who find the port cannot start a match: `SLAPSTICK_JOIN_KEY=...` (or `--join-key`); it
travels inside the encrypted channel. Development UDP is unencrypted; keep it on a trusted network.

## As a service

`deploy/slapstick-server.service` is a hardened systemd user unit, the same shape as
`spooky-kart-server.service` and `prop-hunt-server.service` so all three games can run side by side on
the same box.

```sh
mkdir -p ~/.config/slapstick ~/.local/share/slapstick
printf 'SLAPSTICK_JOIN_KEY=%s\nBLUE_TLS_KEY_FILE=%s\n' "LONG_RANDOM_SECRET" "$HOME/.config/blueengine/server-key.der" > ~/.config/slapstick/server.env
chmod 600 ~/.config/slapstick/server.env
install -D deploy/slapstick-server.service ~/.config/systemd/user/slapstick-server.service
systemctl --user daemon-reload && systemctl --user enable --now slapstick-server
loginctl enable-linger "$USER"        # keep it running when nobody is logged in
```

Open UDP 4102 on the router and firewall only when you want outside players. (4100 is Spooky Kart's and
4101 is Prop Hunt's; keep all three apart.)

## What it records

The netplay kit (`vesper3d::viewer::netplay::server`) appends one line per finished match to
`matches.jsonl` in `--report-dir` generically for any `NetGame`, the same mechanism Spooky Kart and Prop
Hunt use: `NetGame::report()` (this game's final score, winner and whether it ended in sudden death, see
`Sim::report` in `src/sim.rs`) plus each player's network quality (round-trip time, bytes, repeated or
skipped inputs) and the server's own tick times. It is plain JSON lines, so
`python -c "import json,sys; [print(json.loads(l)['report']) for l in open(sys.argv[1])]"` or `jq` reads
it directly.
