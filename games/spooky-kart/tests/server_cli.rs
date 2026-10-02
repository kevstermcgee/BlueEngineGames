//! The server executable and what a hub learns from it. `spooky-kart-server` is the engine's `cli::serve`; these
//! check what is particular to Spooky Kart: the identity it reports (`--info`), the flags old deployments use, the
//! one room setting, and that the content fingerprint players are matched on has not moved.
use spooky_kart::bot::Difficulty;
use spooky_kart::{KartGame, MAX_RACERS};
use std::process::Command;
use vesper3d::viewer::netplay::cli::{build_id, parse_args, Command as Cli, Info, Participants, ServeSpec, SettingDef};
use vesper3d::viewer::netplay::{hub, NetGame, Seat, SettingKind};

const SERVER: &str = env!("CARGO_BIN_EXE_spooky-kart-server");

/// `content_fingerprint()` as published (computed before the hub work). Changing the rules, the track, the lap
/// count or a character's numbers changes it, and then every shipped client is refused by every server: change
/// this value only on purpose, together with a release.
const PUBLISHED_FINGERPRINT: u32 = 0xb4023cbc;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(SERVER).args(args).output().expect("the server binary runs")
}

#[test]
fn the_content_fingerprint_is_the_published_one() {
    assert_eq!(
        spooky_kart::content_fingerprint(),
        PUBLISHED_FINGERPRINT,
        "content_fingerprint() moved: published clients and servers would refuse each other"
    );
    assert_eq!(KartGame::fingerprint(), PUBLISHED_FINGERPRINT);
}

#[test]
fn info_reports_the_game_its_fingerprint_seats_and_the_difficulty_setting() {
    let out = run(&["--info"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    let info = Info::parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(info.game, "spooky-kart");
    assert_eq!(info.fingerprint, KartGame::fingerprint(), "the raw fingerprint, not the folded build");
    assert!(text.contains(&format!("fingerprint={:08x}", KartGame::fingerprint())), "{text}");
    assert!(text.contains("game=spooky-kart"), "{text}");
    assert_eq!(info.max_seats, KartGame::MAX_SEATS);
    assert!(text.contains(&format!("max_seats={MAX_RACERS}")), "{text}");
    assert_eq!(info.build, build_id::<KartGame>());
    assert_eq!(info.build, hub::local_build::<KartGame>());
    assert_eq!(info.tick_hz, 60);
    assert_eq!(
        info.settings,
        vec![SettingDef {
            id: 1,
            name: "difficulty".into(),
            flag: "difficulty".into(),
            kind: SettingKind::Choice,
            min: 0,
            max: 2,
            default: 1,
        }]
    );
    assert!(text.contains("setting=1:difficulty:difficulty:choice:0:2:1"), "{text}");
}

#[test]
fn help_lists_the_flags_deployments_already_use() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let help = String::from_utf8(out.stdout).unwrap();
    for flag in [
        "--listen",
        "--transport",
        "--join-key",
        "--racers N",
        "--auto-start",
        "--seed",
        "--report-dir",
        "--difficulty N",
        "--status-lines",
        "--exit-on-stdin-eof",
    ] {
        assert!(help.contains(flag), "--help lacks {flag}:\n{help}");
    }
    assert!(help.contains("SPOOKY_KART_JOIN_KEY"), "{help}");
    assert!(help.contains("1..=8 (default 8)") || help.contains("1..=8"), "{help}");
}

#[test]
fn an_unknown_flag_is_refused_and_the_old_flags_parse_as_they_did() {
    assert!(!run(&["--no-such-flag"]).status.success());
    // The spec the binary uses, written out: the old command line must mean the same thing.
    let spec = ServeSpec {
        bin_name: "spooky-kart-server",
        about: "",
        default_listen: "0.0.0.0:4100",
        default_report_dir: "spooky-kart-data",
        join_key_env: "SPOOKY_KART_JOIN_KEY",
        participants: Participants::Flag { flag: "racers", min: 1, max: 8, default: 8 },
        default_auto_start: 45,
    };
    let schema: Vec<SettingDef> = KartGame::settings().iter().map(SettingDef::from).collect();
    let args: Vec<String> =
        "--listen 127.0.0.1:5000 --transport development --join-key k --racers 20 --auto-start 10 --report-dir r --seed 7"
            .split(' ')
            .map(String::from)
            .collect();
    let Ok(Cli::Run(o)) = parse_args(&spec, &schema, &args, None) else { panic!("old flags did not parse") };
    assert_eq!((o.listen.as_str(), o.join_key.as_deref(), o.participants), ("127.0.0.1:5000", Some("k"), 8));
    assert_eq!((o.auto_start, o.seed, o.report_dir.to_str()), (10, Some(7), Some("r")));
    // No flags: the old defaults (all eight racers, 45 s auto-start, Medium rivals).
    let Ok(Cli::Run(d)) = parse_args(&spec, &schema, &[], None) else { panic!() };
    assert_eq!((d.participants, d.auto_start, d.settings.clone()), (8, 45, vec![(1, 1)]));
}

#[test]
fn the_difficulty_setting_changes_only_the_rivals_and_medium_is_the_old_behaviour() {
    use spooky_kart::{Driver, Sim};
    use vesper3d::viewer::devkit::Simulation;
    let seats = [Seat { id: 0, choice: 3, name: "A".into() }];
    let hash = |d: Difficulty| {
        let (sim, assigned) = KartGame::start_at(11, &seats, 8, d);
        assert_eq!(assigned.len(), 1);
        (
            sim.karts.iter().map(|k| (k.character, k.driver == Driver::Human, k.skill.to_bits())).collect::<Vec<_>>(),
            sim.difficulty,
        )
    };
    // Medium is exactly what `Sim::with_grid` (the only thing the server used before) builds.
    let (default_sim, _) = KartGame::start_at(11, &seats, 8, Difficulty::Medium);
    let grid: Vec<_> = default_sim.karts.iter().map(|k| (k.character, k.driver)).collect();
    let plain = Sim::with_grid(11, &grid);
    assert_eq!(default_sim.state_hash(), plain.state_hash());
    assert_eq!(hash(Difficulty::Medium).1, Difficulty::Medium);
    // The trait's `start` with nothing configured is Medium too.
    let (via_trait, _) = KartGame::start(11, &seats, 8);
    assert_eq!(via_trait.state_hash(), plain.state_hash());
    // Hard and Easy roll different rival skills from the same seed; the grid itself is the same.
    let (easy, hard) = (hash(Difficulty::Easy), hash(Difficulty::Hard));
    assert_ne!(easy.0, hard.0);
    assert_eq!(
        easy.0.iter().map(|k| (k.0, k.1)).collect::<Vec<_>>(),
        hard.0.iter().map(|k| (k.0, k.1)).collect::<Vec<_>>()
    );
}

#[test]
fn settings_map_to_a_difficulty_and_out_of_range_values_are_refused() {
    use spooky_kart::netgame::difficulty_from_settings as from;
    assert_eq!(from(&[]), Ok(Difficulty::Medium));
    assert_eq!(from(&[(1, 0)]), Ok(Difficulty::Easy));
    assert_eq!(from(&[(1, 1)]), Ok(Difficulty::Medium));
    assert_eq!(from(&[(1, 2)]), Ok(Difficulty::Hard));
    assert!(from(&[(1, 3)]).is_err());
    assert_eq!(from(&[(9, 2)]), Ok(Difficulty::Medium), "another game's id is not ours");
}
