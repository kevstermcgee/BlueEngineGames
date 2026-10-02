//! The dedicated Deadfall server: a lobby, then a match, then results, forever.
//!
//!   deadfall-server [--listen ADDR] [--transport development|production] [--join-key KEY] [--auto-start SECONDS]
//!                   [--report-dir DIR] [--seed N] [--kills N | --minutes N] [--bots] [--skill 0|1|2]
//!                   [--set ID=VALUE] [--status-lines] [--exit-on-stdin-eof] [--info] [--help]
//!
//! The whole command line is the engine's shared server main (`netplay::cli::serve`, ADR 0037). Deadfall's own part is the
//! room settings in `netgame::SETTINGS` (ids 1 `bots`, 2 `kills`, 3 `skill`, 4 `minutes`; `--minutes N` above 0 ends the
//! match on the clock and wins over `--kills`) and the fixed twelve participants. `--info` prints what a hub learns
//! before it starts a room. Default: 40 kills, no bots, only the people who joined play, listening on 0.0.0.0:4100.
//! `development` is raw UDP for a LAN, tests and the hub; `production` is QUIC/TLS 1.3 with a join key for the internet.
use deadfall::netgame::DeadfallGame;
use vesper3d::viewer::netplay::cli::{serve, Participants, ServeSpec};

fn main() -> vesper3d::Result<()> {
    serve::<DeadfallGame>(&ServeSpec {
        bin_name: "deadfall-server",
        about: "The Deadfall dedicated server: a lobby, a match, results, forever.",
        default_listen: "0.0.0.0:4100",
        default_report_dir: "deadfall-data",
        join_key_env: "DEADFALL_JOIN_KEY",
        participants: Participants::Fixed(12),
        default_auto_start: 30,
    })
}
