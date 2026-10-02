//! The race music: which loop to render and how loud each of its three layers should be. Pure (no sound
//! device), so the rules are tested here and `main.rs` only hands the numbers to the `SoundBank`.
use crate::sim::{Phase, Sim, LAPS};
use vesper3d::viewer::devkit::synth::MusicSpec;

/// D minor at 132 bpm, sixteen bars (29.1 s), so it loops on a chord change. Seed 42 was picked from eight
/// candidates for the cleanest loop seam and healthy layer levels (measured, not listened to).
pub const SPEC: MusicSpec = MusicSpec { bpm: 132., bars: 16, root_midi: 50, minor: true, seed: 42 };

/// Another unfinished kart this close (metres) makes it a battle.
pub const BATTLE_METRES: f32 = 14.;

/// Layer targets `[base, melodic, lead]`, each 0 to 1, for the screen being shown. `race` is the race on
/// screen with the human's kart `me`, or `None` on the select screen and lobby.
///
/// Menus and the countdown are the drums and bass alone; racing adds the pad and arpeggio; the final lap or a
/// close battle adds the lead melody; once the player has finished it eases back down.
pub fn layers(race: Option<(&Sim, usize)>) -> [f32; 3] {
    let Some((sim, me)) = race else { return [1., 0., 0.] };
    let Some(kart) = sim.karts.get(me) else { return [1., 0., 0.] };
    match sim.phase {
        Phase::Countdown(_) => [1., 0., 0.],
        Phase::Finished => [1., 0.5, 0.],
        Phase::Racing if kart.finished_tick.is_some() => [1., 0.6, 0.],
        Phase::Racing => {
            let last_lap = kart.lap + 1 >= LAPS;
            let battle = sim
                .karts
                .iter()
                .enumerate()
                .any(|(i, o)| i != me && o.finished_tick.is_none() && (o.pos - kart.pos).length() < BATTLE_METRES);
            [1., 1., if last_lap || battle { 1. } else { 0. }]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Character, Driver};

    fn started(grid: &[(Character, Driver)]) -> Sim {
        let mut sim = Sim::with_grid(1, grid);
        for _ in 0..crate::sim::COUNTDOWN_TICKS {
            sim.step(&Default::default());
        }
        sim
    }

    #[test]
    fn menus_and_the_countdown_are_the_beat_alone() {
        assert_eq!(layers(None), [1., 0., 0.]);
        let sim = Sim::with_grid(1, &[(Character::Vampire, Driver::Human), (Character::Mummy, Driver::Bot)]);
        assert_eq!(layers(Some((&sim, 0))), [1., 0., 0.]);
    }

    #[test]
    fn racing_adds_the_pad_and_the_lead_comes_in_for_a_battle_or_the_last_lap() {
        let mut sim = started(&[(Character::Vampire, Driver::Human), (Character::Mummy, Driver::Bot)]);
        // Far apart on lap one: pad but no lead.
        sim.karts[1].pos.0 += 200.;
        assert_eq!(layers(Some((&sim, 0))), [1., 1., 0.]);
        // A rival alongside: the lead joins.
        sim.karts[1].pos = sim.karts[0].pos;
        sim.karts[1].pos.0 += 5.;
        assert_eq!(layers(Some((&sim, 0)))[2], 1., "a close battle");
        // Alone again but on the last lap.
        sim.karts[1].pos.0 += 200.;
        sim.karts[0].lap = LAPS - 1;
        assert_eq!(layers(Some((&sim, 0)))[2], 1., "the final lap");
    }

    #[test]
    fn finishing_eases_the_music_back_down() {
        let mut sim = started(&[(Character::Vampire, Driver::Human), (Character::Mummy, Driver::Bot)]);
        sim.karts[0].finished_tick = Some(100);
        let l = layers(Some((&sim, 0)));
        assert_eq!(l[2], 0.);
        assert!(l[1] < 1.);
    }

    #[test]
    fn the_loop_is_a_sensible_length() {
        let seconds = SPEC.loop_seconds();
        assert!((28. ..31.).contains(&seconds), "{seconds}");
    }
}
