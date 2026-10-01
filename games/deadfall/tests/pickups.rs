//! Weapons on the ground, on the real map: free slots fill by walking over, full slots ask for E, each gun keeps its
//! own ammunition, and a player carries two firearms, one melee weapon and two grenades at most.
use deadfall::hands::Sel;
use deadfall::input::Input;
use deadfall::sim::{Event, Match, Settings};
use deadfall::weapons::{self, Slot};

fn match_with_one_player() -> Match {
    let (mut m, _) = Match::new(1, &[(0, "A".into()), (1, "B".into())], Settings::default());
    for p in &mut m.players {
        p.protect_until = 0;
    }
    m
}

fn stand_on_loot(m: &mut Match, slot: usize, i: usize) {
    let l = m.world.level.loot[i];
    m.teleport(slot, l.pos, 0., 0.);
}

fn idle(m: &Match, slot: usize, use_press: bool) -> Input {
    let s = m.players[slot].hands.seen;
    Input { yaw: m.players[slot].ctrl.yaw, use_seq: s.use_.wrapping_add(use_press as u8), reload_seq: s.reload, melee_seq: s.melee, drop_seq: s.drop, switch_seq: s.switch, ..Default::default() }
}

fn first_loot_of(m: &Match, slot: Slot) -> usize {
    m.world.level.loot.iter().position(|l| weapons::get(l.weapon).is_some_and(|d| d.slot == slot)).expect("the map has one")
}

fn step(m: &mut Match, press: bool) {
    let inputs = [Some(idle(m, 0, press)), Some(idle(m, 1, false))];
    m.step(&inputs);
}

#[test]
fn walking_over_a_primary_fills_the_empty_slot_with_a_full_gun() {
    let mut m = match_with_one_player();
    let i = first_loot_of(&m, Slot::Primary);
    assert!(m.players[0].inv.primary.is_none());
    stand_on_loot(&mut m, 0, i);
    step(&mut m, false);
    let want = m.world.level.loot[i].weapon;
    let gun = m.players[0].inv.primary.expect("picked up");
    assert_eq!(gun.id, want);
    let def = weapons::get(want).unwrap();
    assert_eq!((gun.mag, gun.reserve), (def.mag, def.reserve), "its own full magazine and reserve");
    assert!(!m.loot[i].available, "the spot is empty now");
    assert!(m.events.iter().any(|e| matches!(e, Event::Pickup { player: 0, .. })));
}

#[test]
fn a_second_primary_needs_e_and_drops_the_first_with_its_ammunition() {
    let mut m = match_with_one_player();
    let mut primaries = (0..m.world.level.loot.len()).filter(|i| weapons::get(m.world.level.loot[*i].weapon).is_some_and(|d| d.slot == Slot::Primary));
    let (a, b) = (primaries.next().unwrap(), primaries.find(|i| m.world.level.loot[*i].weapon != m.world.level.loot[a_id(&m)].weapon).unwrap());
    fn a_id(m: &Match) -> usize {
        (0..m.world.level.loot.len()).find(|i| weapons::get(m.world.level.loot[*i].weapon).is_some_and(|d| d.slot == Slot::Primary)).unwrap()
    }
    stand_on_loot(&mut m, 0, a);
    step(&mut m, false);
    let first = m.players[0].inv.primary.unwrap();
    // Spend a few rounds so the gun's ammunition is distinctive.
    m.players[0].inv.primary.as_mut().unwrap().mag -= 3;
    stand_on_loot(&mut m, 0, b);
    step(&mut m, false);
    assert_eq!(m.players[0].inv.primary.unwrap().id, first.id, "walking over a second gun does not swap");
    step(&mut m, true);
    assert_eq!(m.players[0].inv.primary.unwrap().id, m.world.level.loot[b].weapon, "E swaps");
    let dropped = m.dropped.iter().find(|d| d.gun.id == first.id).expect("the first gun is on the floor");
    assert_eq!(dropped.gun.mag, first.mag - 3, "it keeps the rounds it had");
}

#[test]
fn at_most_two_grenades_and_a_dead_player_drops_what_they_carried() {
    let mut m = match_with_one_player();
    let grenades: Vec<usize> = (0..m.world.level.loot.len()).filter(|i| weapons::get(m.world.level.loot[*i].weapon).is_some_and(|d| d.slot == Slot::Grenade)).collect();
    assert!(grenades.len() >= 3, "the map has grenades");
    for i in grenades.iter().take(3) {
        stand_on_loot(&mut m, 0, *i);
        step(&mut m, true);
    }
    assert_eq!(m.players[0].inv.grenade_count(), 2, "two grenades is the limit");
    let i = first_loot_of(&m, Slot::Primary);
    stand_on_loot(&mut m, 0, i);
    step(&mut m, false);
    m.players[0].health = 0.5;
    m.players[0].armor = 0.;
    let before = m.dropped.len();
    m.damage(0, Some(1), 0, 50., false, 1., vesper3d::math::V::ZERO);
    assert!(!m.players[0].alive);
    assert!(m.dropped.len() >= before + 3, "the rifle and both grenades fell: {} -> {}", before, m.dropped.len());
    let _ = Sel::Primary;
}
