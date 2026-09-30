//! Turning what happens into what you hear: distance, direction, surfaces, footsteps, ambience. The engine
//! plays mono clips with one volume, so direction comes from pre-panned stereo variants (see `audio`).
use super::audio::{self, Sfx, Surface, STEM_COUNT};
use crate::level::{Level, Material};
use vesper3d::math::V;
use vesper3d::viewer::kit::SoundBank;

/// Where the listener is and which way they face.
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    pub pos: V,
    pub yaw: f32,
}

pub struct Audio {
    pub bank: SoundBank,
    /// Sounds waiting for their moment (reload sequences): (seconds from now, cue, position, volume).
    queue: Vec<(f32, Sfx, Option<V>, f32)>,
    /// Footsteps: distance walked since the last one, and which foot, per slot.
    steps: [(f32, bool); 16],
    prev_pos: [Option<V>; 16],
    next_ambient: f32,
    clock: f32,
    started_ambience: bool,
    stems: [f32; STEM_COUNT],
    seed: u32,
}

/// Loudness by distance: full up close, fading to nothing at `reach` metres.
pub fn falloff(distance: f32, reach: f32) -> f32 {
    let d = (distance / reach).clamp(0., 1.);
    (1. - d) * (1. - d) / (1. + distance * 0.04)
}

/// -1 (left) .. 1 (right) for a sound at `at` heard by `l`.
pub fn pan(l: &Listener, at: V) -> f32 {
    let d = V(at.0 - l.pos.0, 0., at.2 - l.pos.2);
    let len = d.length();
    if len < 0.3 {
        return 0.;
    }
    let right = V(l.yaw.cos(), 0., l.yaw.sin());
    ((d.0 * right.0 + d.2 * right.2) / len).clamp(-1., 1.)
}

pub fn surface_of(m: Material) -> Surface {
    match m {
        Material::Concrete | Material::ConcreteDark | Material::Asphalt | Material::Plaster | Material::Brick => Surface::Concrete,
        Material::Metal | Material::RustMetal | Material::Hazard | Material::ContainerRed | Material::ContainerBlue | Material::ContainerGreen | Material::ContainerYellow => Surface::Metal,
        Material::Wood => Surface::Wood,
        Material::Glass => Surface::Glass,
        Material::Gravel => Surface::Gravel,
        Material::Dirt | Material::Grass => Surface::Grass,
        Material::Water => Surface::Water,
    }
}

impl Audio {
    pub async fn start(muted: bool, volume: f32) -> Audio {
        let bank = SoundBank::start(muted, volume, volume, audio::render).await;
        Audio { bank, queue: Vec::new(), steps: [(0., false); 16], prev_pos: [None; 16], next_ambient: 6., clock: 0., started_ambience: false, stems: [0.; STEM_COUNT], seed: 0x9E37_79B9 }
    }

    pub async fn poll(&mut self) {
        self.bank.poll().await;
        if self.bank.ready() && !self.started_ambience {
            self.bank.start_music(); // the ambience beds (wind, machinery, nature, room tone): no music at all
            self.started_ambience = true;
        }
    }

    pub fn set_volume(&mut self, v: f32) {
        self.bank.sfx_volume = v.clamp(0., 1.);
        self.bank.music_volume = v.clamp(0., 1.);
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / 16_777_216.
    }

    /// A sound with no position (your own weapon, the menus).
    pub fn ui(&mut self, sfx: Sfx, volume: f32) {
        let v = if audio::is_directional(sfx) { audio::pan_variant(0.) } else { 0 };
        self.bank.play_variant(audio::index(sfx), v, volume);
    }

    /// A sound at a place in the world, heard by `l`.
    pub fn at(&mut self, sfx: Sfx, pos: V, l: &Listener, volume: f32, reach: f32) {
        let d = (pos - l.pos).length();
        let vol = volume * falloff(d, reach);
        if vol < 0.01 {
            return;
        }
        let variant = if audio::is_directional(sfx) {
            audio::pan_variant(pan(l, pos))
        } else if audio::variant_count(sfx) > 1 {
            (self.rand() * audio::variant_count(sfx) as f32) as usize
        } else {
            0
        };
        self.bank.play_variant(audio::index(sfx), variant, vol);
    }

    /// Play a cue after `delay` seconds.
    pub fn later(&mut self, delay: f32, sfx: Sfx, pos: Option<V>, volume: f32) {
        self.queue.push((delay, sfx, pos, volume));
    }

    /// Reload sounds for a weapon key, spread over `seconds`.
    pub fn reload(&mut self, key: &str, seconds: f32, pos: Option<V>, volume: f32) {
        for (frac, sfx) in audio::reload_sequence(key) {
            self.later(frac * seconds, *sfx, pos, volume);
        }
    }

    /// Footsteps for everyone in earshot, from how far each has walked. `players` are (slot, feet position,
    /// crouched, alive).
    pub fn footsteps(&mut self, level: &Level, players: &[(usize, V, bool, bool)], l: &Listener, mine: Option<usize>) {
        for &(slot, feet, crouched, alive) in players {
            if slot >= 16 {
                continue;
            }
            let prev = self.prev_pos[slot].replace(feet);
            let Some(prev) = prev else { continue };
            let moved = V(feet.0 - prev.0, 0., feet.2 - prev.2).length();
            if !alive || moved > 3. {
                self.steps[slot].0 = 0.;
                continue;
            }
            // Walking quietly (crouched or slow) makes no sound; a run does, every step length.
            let speed = moved * 60.;
            if speed < 3.4 || crouched {
                continue;
            }
            self.steps[slot].0 += moved;
            if self.steps[slot].0 >= 1.9 {
                self.steps[slot].0 = 0.;
                self.steps[slot].1 = !self.steps[slot].1;
                let surface = level
                    .raycast(feet + V(0., 0.3, 0.), V(0., -1., 0.), 1.5)
                    .map_or(Surface::Concrete, |(_, b)| surface_of(level.blocks[b].material));
                let sfx = audio::step(surface, self.steps[slot].1);
                if Some(slot) == mine {
                    self.ui(sfx, 0.35);
                } else {
                    self.at(sfx, feet, l, 0.8, 30.);
                }
            }
        }
    }

    /// Advance timers, release queued sounds, and keep the ambience beds and one-shots going.
    pub fn update(&mut self, dt: f32, l: &Listener, level: &Level) {
        self.clock += dt;
        let mut due = Vec::new();
        self.queue.retain_mut(|q| {
            q.0 -= dt;
            if q.0 <= 0. {
                due.push((q.1, q.2, q.3));
                false
            } else {
                true
            }
        });
        for (sfx, pos, vol) in due {
            match pos {
                Some(p) => self.at(sfx, p, l, vol, 40.),
                None => self.ui(sfx, vol),
            }
        }
        // Ambience beds: wind outside, machinery near the works, nature away from them, room tone indoors.
        let indoors = level.raycast(l.pos, V(0., 1., 0.), 14.).is_some();
        let near_machine = level
            .ambient
            .iter()
            .filter(|a| matches!(a.kind, crate::level::AmbientKind::MachineHum | crate::level::AmbientKind::Steam))
            .map(|a| (a.pos - l.pos).length() / a.radius.max(1.))
            .fold(9., f32::min);
        let near_nature = level
            .ambient
            .iter()
            .filter(|a| matches!(a.kind, crate::level::AmbientKind::Birds | crate::level::AmbientKind::Crickets))
            .map(|a| (a.pos - l.pos).length() / a.radius.max(1.))
            .fold(9., f32::min);
        let target = [
            if indoors { 0.15 } else { 0.55 },
            (1.2 - near_machine * 0.6).clamp(0.12, 0.7),
            if indoors { 0.0 } else { (1.1 - near_nature * 0.5).clamp(0.15, 0.6) },
            if indoors { 0.6 } else { 0.0 },
        ];
        self.stems = target;
        self.bank.update_music(dt, &self.stems);
        // One-shot noises of the place, now and then, from the nearest matching source.
        self.next_ambient -= dt;
        if self.next_ambient <= 0. {
            self.next_ambient = 5. + self.rand() * 9.;
            let pick = (self.rand() * level.ambient.len().max(1) as f32) as usize;
            if let Some(a) = level.ambient.get(pick) {
                let sfx = match a.kind {
                    crate::level::AmbientKind::MachineHum | crate::level::AmbientKind::MetalCreak => {
                        if self.rand() < 0.5 {
                            Sfx::AmbClang
                        } else {
                            Sfx::AmbCreak
                        }
                    }
                    crate::level::AmbientKind::Dripping => Sfx::AmbDrip,
                    crate::level::AmbientKind::Steam => Sfx::AmbSteam,
                    crate::level::AmbientKind::Birds => {
                        if self.rand() < 0.7 {
                            Sfx::AmbBird
                        } else {
                            Sfx::AmbPigeon
                        }
                    }
                    crate::level::AmbientKind::WindGap | crate::level::AmbientKind::Crickets => Sfx::AmbCreak,
                };
                let d = (a.pos - l.pos).length();
                if d < a.radius * 2.5 {
                    self.at(sfx, a.pos, l, 0.5, a.radius * 2.5);
                }
            }
        }
    }

    pub fn clear_match_state(&mut self) {
        self.queue.clear();
        self.prev_pos = [None; 16];
        self.steps = [(0., false); 16];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_fade_with_distance_and_pan_follows_the_listener() {
        assert!(falloff(1., 100.) > falloff(50., 100.));
        assert_eq!(falloff(120., 100.), 0.);
        let l = Listener { pos: V(0., 0., 0.), yaw: 0. };
        // Facing -Z: +X is to the right.
        assert!(pan(&l, V(10., 0., 0.)) > 0.9);
        assert!(pan(&l, V(-10., 0., 0.)) < -0.9);
        assert!(pan(&l, V(0., 0., -10.)).abs() < 0.05);
        let turned = Listener { pos: V(0., 0., 0.), yaw: std::f32::consts::PI };
        assert!(pan(&turned, V(10., 0., 0.)) < -0.9, "turned around, +X is on the left");
    }
}
