# Hosting a Spooky Kart server

One small process runs the lobby, the races and the results screen for up to eight players; empty grid
slots are filled with bots, and a player who drops out is replaced by a bot. On the reference machine (an
Intel N97 mini PC) it used about 1.5% of one core and under 4 MB of memory with two players; see
`docs/perf` in BlueEngine (`python tools/perf.py report`, metrics starting `kart_`).

## Build and run

```sh
cargo build --release --no-default-features --bin spooky-kart-server
install -D target/release/spooky-kart-server ~/.local/bin/spooky-kart-server
spooky-kart-server --listen 0.0.0.0:4100 --transport development          # LAN or testing: raw UDP
```

Players connect with `spooky-kart --connect SERVER_IP:4100` (add `--transport production` and
`--join-key KEY` for a production server). Use UDP port 4100.

## Production (the internet)

`--transport production` is QUIC over TLS 1.3, the engine's own secure transport, with a pinned
certificate. Provision the server key and certificate outside the repository:

```sh
export BLUE_TLS_KEY_FILE=~/.config/blueengine/server-key.der     # PKCS#8 DER private key, mode 600
export BLUE_TLS_CERT_FILE=~/.config/blueengine/server-cert.der   # the matching public certificate
```

To make a certificate of your own (the name must be `feta.local`). It must **not** be flagged as a CA: the TLS
library refuses a CA certificate as a server's identity, and the failure only shows as "Could not reach the server".

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
so strangers who find the port cannot start races: `SPOOKY_KART_JOIN_KEY=...` (or `--join-key`); it travels
inside the encrypted channel. Development UDP is unencrypted; keep it on a trusted network. Verified: the real server and bot clients over
QUIC/TLS, including a wrong join key being refused.

## As a service

`deploy/spooky-kart-server.service` is a hardened systemd user unit.

```sh
mkdir -p ~/.config/spooky-kart ~/.local/share/spooky-kart
printf 'SPOOKY_KART_JOIN_KEY=%s\nBLUE_TLS_KEY_FILE=%s\n' "LONG_RANDOM_SECRET" "$HOME/.config/blueengine/server-key.der" > ~/.config/spooky-kart/server.env
chmod 600 ~/.config/spooky-kart/server.env
install -D deploy/spooky-kart-server.service ~/.config/systemd/user/spooky-kart-server.service
systemctl --user daemon-reload && systemctl --user enable --now spooky-kart-server
loginctl enable-linger "$USER"        # keep it running when nobody is logged in
```

Open UDP 4100 on the router and firewall only when you want outside players.

## What it records

Every finished race appends one line to `races.jsonl` in the report directory: the results, each racer's
drifting, wall hits, collisions, perk uses and speeds, and each player's network quality (round-trip time,
bytes, repeated or skipped inputs) plus the server's tick times. Read it with:

```sh
python tools/analyze.py ~/.local/share/spooky-kart/races.jsonl
```

The character table shows who is strong or weak (rebalance `src/character.rs`); the network table shows lag
and loss. `python tools/load_test.py` races bot clients against a real server and prints the same numbers as
metric rows.
