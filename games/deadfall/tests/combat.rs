//! The rules of a fight, through the public simulation API.
use deadfall::hands::Sel;
use deadfall::input::{Input, FIRE};
use deadfall::sim::{hit, world_for, EndRule, Event, Match, Phase, Settings, RESPAWN_TICKS};
use deadfall::weapons;
use vesper3d::math::V;

fn arena() -> std::sync::Arc<deadfall::sim::World> {
    static A: std::sync::OnceLock<std::sync::Arc<deadfall::sim::World>> = std::sync::OnceLock::new();
    A.get_or_init(|| world_for(deadfall::level::placeholder())).clone()
}

fn duel(settings: Settings) -> Match {
    let (m, slots) = Match::new_in(arena(), 7, &[(0, "Alpha".into()), (1, "Bravo".into())], settings);
    assert_eq!(slots, vec![0, 1]);
    m
}

fn facing(from: V, to: V) -> (f32, f32) {
    let d = to - from;
    (d.0.atan2(-d.2), (d.1).atan2((d.0 * d.0 + d.2 * d.2).sqrt()))
}

/// Put A at `a` and B at `b` (feet), A looking at B's chest.
fn stage(m: &mut Match, a: V, b: V) {
    let eye = |f: V| V(f.0, f.1 + 1.68, f.2);
    let (yaw, pitch) = facing(eye(a), eye(b) - V(0., 0.3, 0.));
    m.teleport(0, a, yaw, pitch);
    m.teleport(1, b, yaw + std::f32::consts::PI, 0.);
    m.players[0].protect_until = 0;
    m.players[1].protect_until = 0;
}

fn input_for(m: &Match, slot: usize, buttons: u8) -> Input {
    let p = &m.players[slot];
    let seen = p.hands.seen;
    Input {
        yaw: p.ctrl.yaw,
        pitch: p.ctrl.pitch,
        buttons,
        seen_tick: m.tick as u16,
        reload_seq: seen.reload,
        use_seq: seen.use_,
        melee_seq: seen.melee,
        drop_seq: seen.drop,
        switch_seq: seen.switch,
        ..Default::default()
    }
}

fn run(m: &mut Match, ticks: u32, a: impl Fn(&Match) -> Input, b: impl Fn(&Match) -> Input) {
    for _ in 0..ticks {
        let inputs = [Some(a(m)), Some(b(m))];
        m.step(&inputs);
    }
}

fn killed(m: &Match) -> bool {
    m.players[1].deaths > 0
}

#[test]
fn a_pistol_kills_a_standing_enemy_and_scores() {
    let mut m = duel(Settings::default());
    stage(&mut m, V(-20., 0., 5.), V(-20., 0., -8.));
    let mut pressed = false;
    // Tap the trigger (semi-automatic needs a fresh press each shot).
    for t in 0..(60 * 6) {
        let buttons = if t % 6 < 3 { FIRE } else { 0 };
        let mut i = input_for(&m, 0, buttons);
        i.seen_tick = m.tick as u16;
        m.step(&[Some(i), Some(input_for(&m, 1, 0))]);
        pressed |= m.events.iter().any(|e| matches!(e, Event::Kill { killer: 0, victim: 1, .. }));
        m.events.clear();
        if killed(&m) {
            break;
        }
    }
    assert!(killed(&m), "six seconds of pistol fire at 13 m should kill: health {}", m.players[1].health);
    assert!(pressed);
    assert_eq!(m.scores, [1, 0]);
    assert_eq!(m.players[0].kills, 1);
    assert!(m.players[0].inv.secondary.unwrap().mag < 17, "shots cost rounds");
}

#[test]
fn a_dead_player_returns_after_the_killcam_and_is_protected_for_a_moment() {
    let mut m = duel(Settings::default());
    stage(&mut m, V(-20., 0., 5.), V(-20., 0., -8.));
    m.players[1].health = 1.;
    m.players[1].armor = 0.;
    for t in 0..120 {
        let i = input_for(&m, 0, if t % 6 < 3 { FIRE } else { 0 });
        m.step(&[Some(i), Some(input_for(&m, 1, 0))]);
        if killed(&m) {
            break;
        }
    }
    assert!(killed(&m));
    let died = m.players[1].died_at;
    assert_eq!(m.players[1].killed_by, Some(0));
    run(&mut m, RESPAWN_TICKS - 2, |m| input_for(m, 0, 0), |m| input_for(m, 1, 0));
    assert!(!m.players[1].alive, "still in the killcam");
    run(&mut m, 4, |m| input_for(m, 0, 0), |m| input_for(m, 1, 0));
    assert!(m.players[1].alive, "back after {} ticks", m.tick - died);
    assert_eq!(m.players[1].health, 100.);
    assert!(m.tick < m.players[1].protect_until, "spawn protection");
}

#[test]
fn walls_stop_bullets_and_teammates_cannot_hurt_each_other() {
    let (mut m, _) =
        Match::new_in(arena(), 3, &[(0, "A".into()), (0, "B".into()), (1, "C".into())], Settings::default());
    // A and B are teammates; the crate in the middle of the placeholder map is between A and C.
    m.teleport(0, V(0., 0., 10.), 0., 0.);
    m.teleport(1, V(0., 0., 7.), 0., 0.);
    m.teleport(2, V(0., 0., -10.), 0., 0.);
    for p in &mut m.players {
        p.protect_until = 0;
    }
    let (y, p) = facing(V(0., 1.68, 10.), V(0., 1.4, -10.));
    let fire = Input { yaw: y, pitch: p, buttons: FIRE, ..Default::default() };
    m.step(&[Some(fire), Some(input_for(&m, 1, 0)), Some(input_for(&m, 2, 0))]);
    assert_eq!(m.players[2].health, 100., "the crate is in the way");
    // A shot through teammate B does nothing to B.
    let shot = m.events.iter().find_map(|e| match e {
        Event::Shot { hit: h, .. } => Some(*h),
        _ => None,
    });
    assert_eq!(shot, Some(hit::WORLD));
    assert_eq!(m.players[1].health, 100.);
}

#[test]
fn an_empty_magazine_reloads_by_itself_and_ammunition_adds_up() {
    let mut m = duel(Settings::default());
    stage(&mut m, V(-20., 0., 5.), V(-20., 0., -8.));
    m.players[1].armor = 100.;
    m.players[1].protect_until = u32::MAX;
    let start = m.players[0].inv.secondary.unwrap();
    let mut t = 0;
    while m.players[0].inv.secondary.unwrap().mag > 0 && t < 60 * 20 {
        let i = input_for(&m, 0, if t % 10 < 4 { FIRE } else { 0 });
        m.step(&[Some(i), Some(input_for(&m, 1, 0))]);
        t += 1;
    }
    assert_eq!(m.players[0].inv.secondary.unwrap().mag, 0);
    // One more pull on the empty weapon starts the reload.
    run(&mut m, 40, |m| input_for(m, 0, FIRE), |m| input_for(m, 1, 0));
    run(&mut m, 40, |m| input_for(m, 0, 0), |m| input_for(m, 1, 0));
    run(&mut m, 120, |m| input_for(m, 0, 0), |m| input_for(m, 1, 0));
    let g = m.players[0].inv.secondary.unwrap();
    assert_eq!(g.mag, start.mag, "a full magazine after the reload");
    assert_eq!(start.reserve - g.reserve, start.mag, "exactly one magazine came from the reserve");
}

#[test]
fn a_moving_target_is_hit_where_the_shooter_saw_it() {
    // Lag compensation: the shooter's view is 8 ticks old. The target has since walked on.
    let setup = || {
        let mut m = duel(Settings::default());
        stage(&mut m, V(-20., 0., 5.), V(-20., 0., -8.));
        m.players[1].armor = 0.;
        m.players[1].protect_until = 0;
        for _ in 0..12 {
            m.step(&[Some(input_for(&m, 0, 0)), Some(input_for(&m, 1, 0))]);
        }
        let mut walk = input_for(&m, 1, 0);
        walk.set_axes(-1., 0.);
        for _ in 0..8 {
            m.step(&[Some(input_for(&m, 0, 0)), Some(walk)]);
        }
        (m, walk)
    };
    let (mut m, walk) = setup();
    // Aim at where the target was when the shooter's screen last updated (8 ticks before the coming tick).
    let then = m.history[m.history.len() - 8][1].eye;
    let moved = (m.players[1].eye().0 - then.0).abs();
    assert!(moved > 0.3, "the target has moved {moved} m since");
    let (yaw, pitch) = facing(V(-20., 1.68, 5.), V(then.0, then.1 - 0.35, then.2));
    let shot = Input { yaw, pitch, buttons: FIRE, seen_tick: (m.tick + 1 - 8) as u16, ..Default::default() };
    let before = m.players[1].health;
    m.step(&[Some(shot), Some(walk)]);
    assert!(m.players[1].health < before, "hit through lag compensation");
    // The same aim claiming to have seen the present misses: the target is no longer there.
    let (mut m2, walk) = setup();
    let late = Input { seen_tick: (m2.tick + 1) as u16, ..shot };
    let before = m2.players[1].health;
    m2.step(&[Some(late), Some(walk)]);
    assert_eq!(m2.players[1].health, before, "without compensation the shot misses the moved target");
}

#[test]
fn the_first_team_to_the_kill_target_wins_and_the_match_ends() {
    let mut m = duel(Settings { end: EndRule::Kills { target: 1 }, ..Default::default() });
    stage(&mut m, V(-20., 0., 5.), V(-20., 0., -8.));
    m.players[1].health = 1.;
    m.players[1].armor = 0.;
    for t in 0..120 {
        let i = input_for(&m, 0, if t % 6 < 3 { FIRE } else { 0 });
        m.step(&[Some(i), Some(input_for(&m, 1, 0))]);
        if matches!(m.phase, Phase::Over { .. }) {
            break;
        }
    }
    assert!(matches!(m.phase, Phase::Over { winner: Some(deadfall::Team::Ironclad), .. }));
    assert!(!m.is_over(), "it lingers so the last kill is seen");
    run(&mut m, 250, |m| input_for(m, 0, 0), |m| input_for(m, 1, 0));
    assert!(m.is_over());
}

#[test]
fn a_timed_match_ends_when_the_clock_runs_out() {
    let mut m = duel(Settings { end: EndRule::Time { minutes: 1 }, ..Default::default() });
    m.scores = [3, 5];
    run(&mut m, 60 * 60 + 2, |m| input_for(m, 0, 0), |m| input_for(m, 1, 0));
    assert!(matches!(m.phase, Phase::Over { winner: Some(deadfall::Team::Nightwatch), .. }));
    assert_eq!(m.seconds_left(), Some(0.));
}

#[test]
fn a_frag_hurts_in_the_open_and_a_wall_shelters() {
    let frag = weapons::id_of("frag").unwrap();
    let (mut m, _) =
        Match::new_in(arena(), 9, &[(0, "T".into()), (1, "Open".into()), (1, "Sheltered".into())], Settings::default());
    m.players[0].inv.grenades = [frag, 0];
    for p in &mut m.players {
        p.protect_until = 0;
        p.armor = 0.;
    }
    // Thrower far away; the open target 4 m from the blast; the sheltered one just behind the crate (it is 6 m
    // across, so the blast is 0.6 m in front of one face and the target 0.3 m behind the other).
    m.teleport(0, V(20., 0., 20.), 0., 0.);
    m.teleport(1, V(0., 0., -7.6), 0., 0.);
    m.teleport(2, V(0., 0., 3.3), 0., 0.);
    let mut i = input_for(&m, 0, 0);
    i.switch_seq = i.switch_seq.wrapping_add(1);
    i.switch_to = 3;
    m.step(&[Some(i), None, None]);
    for _ in 0..40 {
        let a = input_for(&m, 0, 0);
        m.step(&[Some(a), None, None]);
    }
    for _ in 0..10 {
        let a = input_for(&m, 0, FIRE);
        m.step(&[Some(a), None, None]);
    }
    let a = input_for(&m, 0, 0);
    m.step(&[Some(a), None, None]);
    assert_eq!(m.projectiles.len(), 1, "the grenade is in the air");
    // Put it where the test needs it, one tick before the fuse runs out.
    let fuse = m.projectiles[0].fuse;
    while m.projectiles[0].age + 2 < fuse {
        let a = input_for(&m, 0, 0);
        m.step(&[Some(a), None, None]);
    }
    m.projectiles[0].pos = V(0., 0.3, -3.6);
    m.projectiles[0].vel = V(0., 0., 0.);
    for _ in 0..4 {
        let a = input_for(&m, 0, 0);
        m.step(&[Some(a), None, None]);
    }
    assert!(m.projectiles.is_empty(), "it went off");
    assert!(m.players[1].health < 100., "the open target took blast damage: {}", m.players[1].health);
    assert_eq!(m.players[2].health, 100., "the crate shelters the other");
}
