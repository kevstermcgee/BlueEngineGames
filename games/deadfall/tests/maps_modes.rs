//! Objective rules and traversable maps, independent of rendering or real time.
use deadfall::{
    input::{Input, USE_HELD},
    maps::MapId,
    modes::GameMode,
    sim::{self, EndRule, Match, Phase, Settings},
};
use vesper3d::math::V;
fn game(mode: GameMode) -> Match {
    Match::new(
        14,
        &[(0, "Kevin".into()), (0, "Cousin".into())],
        Settings { mode, map: MapId::Switchyard, duel: true, end: EndRule::Kills { target: 10 }, ..Default::default() },
    )
    .0
}
fn place(m: &mut Match, slot: usize, feet: V) {
    m.players[slot].ctrl = sim::new_body(feet, 0.);
}
#[test]
fn all_maps_have_reachable_spawns_weapons_and_objectives() {
    for map in MapId::ALL {
        let w = sim::world_on(map);
        assert_eq!(w.level.name, map.name());
        assert!(w.level.loot.len() <= 64);
        let origin = w.level.spawns[0][0].pos;
        for pos in w
            .level
            .spawns
            .iter()
            .flatten()
            .map(|s| s.pos)
            .chain(w.level.loot.iter().map(|l| l.pos))
            .chain(map.bases())
            .chain(map.sites())
        {
            assert!(w.nav.path(origin, pos).is_some(), "{} unreachable {:?}", map.name(), pos);
        }
        assert!(w.level.spawns.iter().all(|s| s.len() >= 8));
        for s in w.level.spawns.iter().flatten() {
            let mut body = sim::new_body(s.pos, s.yaw);
            let start = body.position;
            for _ in 0..10 {
                sim::step_body(&mut body, &Input::default(), 1., &w.colliders);
            }
            assert!(
                (body.position - start).length() < 0.12,
                "{} spawn {:?} moved to {:?}",
                map.name(),
                start,
                body.position
            );
        }
    }
}
#[test]
fn a_duel_balances_two_humans_and_never_fills_with_bots() {
    let m = Match::new(
        1,
        &[(0, "A".into()), (0, "B".into()), (0, "C".into())],
        Settings { duel: true, bots: true, ..Default::default() },
    )
    .0;
    assert_eq!(m.players.len(), 2);
    assert!(m.enemies(0, 1));
    assert!(m.players.iter().all(|p| p.human));
}
#[test]
fn flags_must_be_carried_home_and_your_own_flag_must_be_home() {
    let mut m = game(GameMode::CaptureFlag);
    let bases = m.settings.map.bases();
    place(&mut m, 0, bases[1]);
    m.step(&[Some(Input::default()), Some(Input::default())]);
    assert_eq!(m.objective.flags[1].carrier, 0);
    m.objective.flags[0].dropped_at = 1;
    m.objective.flags[0].pos = V(20., 0., 0.);
    place(&mut m, 0, bases[0]);
    m.step(&[Some(Input::default()), Some(Input::default())]);
    assert_eq!(m.scores, [0, 0]);
    m.objective.flags[0].dropped_at = 0;
    m.objective.flags[0].pos = bases[0];
    m.step(&[Some(Input::default()), Some(Input::default())]);
    assert_eq!(m.scores, [1, 0]);
    assert_eq!(m.objective.flags[1].carrier, 255);
}
#[test]
fn dead_carriers_drop_flags_and_dropped_flags_return_automatically() {
    let mut m = game(GameMode::CaptureFlag);
    m.objective.flags[1].carrier = 0;
    place(&mut m, 0, V(10., 0., 0.));
    m.players[0].alive = false;
    m.players[0].died_at = m.tick;
    m.step(&[]);
    assert_eq!(m.objective.flags[1].carrier, 255);
    assert_ne!(m.objective.flags[1].dropped_at, 0);
    m.tick += 1200;
    m.players.iter_mut().for_each(|p| p.alive = false);
    m.step(&[]);
    assert_eq!(m.objective.flags[1].pos, m.settings.map.bases()[1]);
}
#[test]
fn search_has_no_respawn_and_a_planted_bomb_survives_attacker_elimination() {
    let mut m = game(GameMode::SearchDestroy);
    m.objective.phase = 1;
    m.objective.deadline = 10000;
    m.objective.carrier = 0;
    let site = m.settings.map.sites()[0];
    place(&mut m, 0, site);
    let hold = Input { buttons: USE_HELD, ..Default::default() };
    for _ in 0..180 {
        m.step(&[Some(hold), Some(Input::default())]);
    }
    assert_eq!(m.objective.phase, 2);
    m.players[0].alive = false;
    m.players[0].died_at = m.tick;
    for _ in 0..300 {
        m.step(&[Some(Input::default()), Some(Input::default())]);
    }
    assert!(!m.players[0].alive);
    assert_eq!(m.objective.phase, 2);
    m.tick = m.objective.deadline;
    m.step(&[]);
    assert_eq!(m.scores, [1, 0]);
    assert_eq!(m.objective.phase, 3);
    assert!(m.events.iter().any(|e| matches!(e, deadfall::sim::Event::Blast { pos, .. } if *pos == site)));
}
#[test]
fn unplanted_elimination_does_not_detonate_a_bomb() {
    let mut m = game(GameMode::SearchDestroy);
    m.objective.phase = 1;
    m.objective.deadline = 10000;
    m.players[1].alive = false;
    m.step(&[]);
    assert_eq!(m.scores, [1, 0]);
    assert!(!m.events.iter().any(|e| matches!(e, deadfall::sim::Event::Blast { .. })));
}
#[test]
fn defuse_requires_a_continuous_hold_and_sides_alternate() {
    let mut m = game(GameMode::SearchDestroy);
    m.objective.phase = 2;
    m.objective.deadline = 10000;
    m.objective.bomb = m.settings.map.sites()[0];
    let bomb = m.objective.bomb;
    place(&mut m, 1, bomb);
    let hold = Input { buttons: USE_HELD, ..Default::default() };
    for _ in 0..100 {
        m.step(&[None, Some(hold)]);
    }
    assert_eq!(m.objective.progress, 100);
    m.step(&[None, Some(Input::default())]);
    assert_eq!(m.objective.progress, 0);
    for _ in 0..300 {
        m.step(&[None, Some(hold)]);
    }
    assert_eq!(m.scores, [0, 1]);
    assert_eq!(m.objective.phase, 3);
    m.tick = m.objective.deadline;
    m.step(&[]);
    assert_eq!(m.objective.round, 2);
    assert_eq!(m.objective.carrier, 1);
    assert!(m.players.iter().all(|p| p.alive));
}
#[test]
fn free_for_all_uses_individual_scores_even_with_identical_team_cosmetics() {
    let mut m = game(GameMode::FreeForAll);
    m.players[1].team = m.players[0].team;
    assert!(m.enemies(0, 1));
    m.players[1].kills = 10;
    m.step(&[]);
    assert!(matches!(m.phase, Phase::Over { .. }));
    assert_eq!(m.winner_slot, 1);
}
#[test]
fn leaving_a_duel_awards_a_forfeit_without_an_unrequested_bot() {
    let mut m = game(GameMode::TeamDeathmatch);
    m.release(1);
    assert!(matches!(m.phase, Phase::Over { .. }));
    assert_eq!(m.winner_slot, 0);
    assert!(m.players[1].bot.is_none());
}

#[test]
fn navigation_crosses_a_narrow_roof_joint_but_never_a_wall() {
    use deadfall::level::{Builder, Material};
    let mut b = Builder::new("Joint", 4., 4.);
    b.solid(-3., -2., -0.1, 2., 0., 2.6, Material::Concrete);
    b.solid(0.1, -2., 3., 2., 0., 2.6, Material::Concrete);
    let from = V(-2., 2.6, 0.);
    let to = V(2., 2.6, 0.);
    assert!(deadfall::nav::Nav::build(&b.level).path(from, to).is_some());
    b.solid(-0.1, -3., 0.1, 3., 2.6, 3., Material::Concrete);
    assert!(deadfall::nav::Nav::build(&b.level).path(from, to).is_none());
}

#[test]
fn a_bot_can_navigate_capture_and_plant_on_every_map() {
    for map in MapId::ALL {
        for mode in [GameMode::CaptureFlag, GameMode::SearchDestroy] {
            // Isolate navigation/objective completion from whether a competitive twelve-bot game ends in a stalemate.
            let seats = if mode == GameMode::CaptureFlag {
                vec![(0, "Runner".into())]
            } else {
                vec![(0, "Runner".into()), (1, "Passive defender".into())]
            };
            let (mut m, _) = Match::new(
                19,
                &seats,
                Settings {
                    map,
                    mode,
                    bots: false,
                    objective_target: 1,
                    end: EndRule::Time { minutes: 3 },
                    ..Default::default()
                },
            );
            let mut rng = vesper3d::viewer::devkit::Rng::new(19);
            m.players[0].human = false;
            m.players[0].bot = Some(deadfall::bots::BotState::new(1, &mut rng));
            if m.players.len() > 1 {
                m.players[1].health = 1_000_000.;
            }
            let clock = std::time::Instant::now();
            let mut ticks = 0;
            let mut interacted = false;
            for _ in 0..60 * 180 {
                m.step(&[]);
                ticks += 1;
                interacted |= m.objective.flags.iter().any(|f| f.carrier != 255) || m.objective.phase == 2;
                assert!(m.players.iter().all(|p| p.ctrl.position.0.is_finite()
                    && p.ctrl.position.1.is_finite()
                    && p.ctrl.position.2.is_finite()));
                m.events.clear();
                if m.phase != Phase::Live {
                    break;
                }
            }
            println!(
                "{} / {}: scores {:?}, interacted {}, {:.3} ms/tick",
                map.name(),
                mode.name(),
                m.scores,
                interacted,
                clock.elapsed().as_secs_f64() * 1000. / ticks as f64
            );
            assert!(
                interacted && m.scores.iter().any(|s| *s > 0),
                "{} / {} bot could not complete an objective",
                map.name(),
                mode.name()
            );
        }
    }
}
