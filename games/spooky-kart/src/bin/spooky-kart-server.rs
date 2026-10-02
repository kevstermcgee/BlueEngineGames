//! The dedicated Spooky Kart server: a lobby, then a race, then results, forever.
//!
//! The whole `main` is the engine's `netplay::cli::serve` (flags, `--info`, `--status-lines`, clean stop), so the
//! hub (`be2-hub`, docs/HOSTING.md) can run it as a room. By hand it still takes:
//!
//!   spooky-kart-server [--listen ADDR] [--transport development|production] [--join-key KEY]
//!                      [--racers N] [--auto-start SECONDS] [--report-dir DIR] [--seed N] [--difficulty 0|1|2]
//!
//! `development` is raw UDP for a LAN, testing or a hub's rooms. `production` is QUIC/TLS 1.3 with the engine's pinned
//! certificate (`BLUE_TLS_KEY_FILE`, `BLUE_TLS_CERT_FILE`); use it for anything reachable from the internet, together
//! with a join key (`--join-key` or SPOOKY_KART_JOIN_KEY). `--difficulty` is how hard the bots on the grid drive
//! (0 Easy, 1 Medium, the default, 2 Hard). Every finished race appends a line to `DIR/matches.jsonl`: results,
//! per-character statistics and network quality, the data used to tune the game. `--help` lists every flag.
use spooky_kart::KartGame;
use vesper3d::viewer::netplay::cli::{serve, Participants, ServeSpec};

fn main() -> vesper3d::Result<()> {
    serve::<KartGame>(&ServeSpec {
        bin_name: "spooky-kart-server",
        about: "The Spooky Kart dedicated server: lobby, race and results for up to eight racers; bots fill the grid.",
        default_listen: "0.0.0.0:4100",
        default_report_dir: "spooky-kart-data",
        join_key_env: "SPOOKY_KART_JOIN_KEY",
        participants: Participants::Flag { flag: "racers", min: 1, max: 8, default: 8 },
        default_auto_start: 45,
    })
}
