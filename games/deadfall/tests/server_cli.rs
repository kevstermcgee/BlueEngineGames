//! `deadfall-server --info` is what the engine hub reads before it runs a room: the game id, the raw join
//! fingerprint, the seats and the room settings (ids and names are stable: they are on the hub's wire).
use deadfall::netgame::DeadfallGame;
use std::process::Command;
use vesper3d::viewer::netplay::cli::{build_id, Info};
use vesper3d::viewer::netplay::NetGame;

fn run(args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_deadfall-server")).args(args).output().unwrap();
    (out.status.success(), String::from_utf8(out.stdout).unwrap())
}

#[test]
fn info_prints_the_game_the_raw_fingerprint_the_seats_and_the_four_settings() {
    let (ok, text) = run(&["--info"]);
    assert!(ok);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "game=deadfall");
    assert_eq!(lines[1], format!("fingerprint={:08x}", DeadfallGame::fingerprint()));
    assert_eq!(lines[1], "fingerprint=2bafe9c8", "the shipped clients compare this raw value");
    assert_eq!(lines[2], format!("build={:08x}", build_id::<DeadfallGame>()));
    assert_eq!(lines[3], "max_seats=12");
    assert_eq!(lines[4], "tick_hz=60");
    assert_eq!(
        &lines[5..],
        [
            "setting=1:bots:bots:bool:0:1:0",
            "setting=2:kills:kills:int:1:500:40",
            "setting=3:skill:skill:choice:0:2:1",
            "setting=4:minutes:minutes:int:0:60:0",
        ]
    );
    assert_eq!(Info::parse(&text), Ok(Info::of::<DeadfallGame>()));
}

#[test]
fn help_lists_the_old_flags_and_a_bad_setting_is_refused() {
    let (ok, help) = run(&["--help"]);
    assert!(ok);
    for flag in ["--bots", "--kills N", "--skill N", "--minutes N", "--status-lines", "--exit-on-stdin-eof", "DEADFALL_JOIN_KEY"] {
        assert!(help.contains(flag), "{flag} missing from\n{help}");
    }
    let bad = Command::new(env!("CARGO_BIN_EXE_deadfall-server")).args(["--kills", "0"]).output().unwrap();
    assert!(!bad.status.success(), "a kill target of 0 is refused, not silently raised");
}
