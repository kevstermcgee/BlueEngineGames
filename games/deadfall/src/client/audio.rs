//! Every sound of Deadfall, synthesised in code (no music, no voices: natural sounds of the place,
//! weapons, explosions and a few quiet UI clicks).
//!
//! [`render`] returns the engine's `Rendered` bank. Sounds are addressed by [`index`] (`Sfx as usize`).
//!
//! **Stereo.** The engine's backend (quad-snd 0.2.8, `mixer::load_samples_from_file`) accepts 1 or 2 channel
//! WAV, duplicates mono to both channels and resamples everything to 44.1 kHz with a nearest-neighbour
//! resampler. So directional cues are rendered as [`PAN_STEPS`] stereo variants (hard left .. centre ..
//! hard right): call `play_variant(index(sfx), pan_variant(pan), volume)`. Everything is rendered at
//! 22.05 kHz (an exact 2x for that resampler, and half the memory).
//!
//! Directional cues have `PAN_STEPS` variants ([`is_directional`]); the ambience one-shots
//! ([`Sfx::AmbClang`] .. [`Sfx::AmbPigeon`]) have [`AMB_TAKES`] variants (different takes); everything else has
//! 1. Your own weapon and UI cues are not directional: play variant 0 (or the centre pan).
//!
//! Stems (ambience bed, seamless loops of [`STEM_SECONDS`] seconds, mono): 0 wind over the yard, 1 distant
//! machinery, 2 nature (crickets, birds, leaves), 3 interior room tone.
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};
use vesper3d::viewer::devkit::synth::{wav_bytes, wav_bytes_stereo};
use vesper3d::viewer::devkit::Rng;
use vesper3d::viewer::kit::Rendered;

/// Sample rate of every clip. Exactly half of the engine's 44.1 kHz so its resampler just doubles samples.
pub const RATE: u32 = 22_050;
const SR: f32 = RATE as f32;
/// Number of stereo pan variants of a directional cue (odd: the middle one is centred).
pub const PAN_STEPS: usize = 7;
/// Number of takes of each ambience one-shot.
pub const AMB_TAKES: usize = 3;
/// Length of each ambience stem in seconds.
pub const STEM_SECONDS: f32 = 16.0;
/// Number of ambience stems.
pub const STEM_COUNT: usize = 4;
/// Stem indices.
pub const STEM_WIND: usize = 0;
pub const STEM_MACHINERY: usize = 1;
pub const STEM_NATURE: usize = 2;
pub const STEM_INTERIOR: usize = 3;
const SEED: u64 = 0xDEAD_FA11_5EED;

macro_rules! sfx_list {
    ($($v:ident),* $(,)?) => {
        /// Every cue. `Sfx as usize` (= [`index`]) is the sound index in the `Rendered` bank; the order is
        /// stable, new cues are appended.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(usize)]
        pub enum Sfx { $($v),* }
        impl Sfx {
            /// All cues in index order.
            pub const ALL: &'static [Sfx] = &[$(Sfx::$v),*];
            /// The variant name, e.g. `"ShotK47"`.
            pub fn name(self) -> &'static str {
                match self { $(Sfx::$v => stringify!($v)),* }
            }
        }
    };
}

sfx_list! {
    // Gunshots, one per firearm of the roster (ids 1..=25), directional.
    ShotK9, ShotM45, ShotHc50, ShotRv357, ShotMp9, ShotUmp, ShotPdw, ShotVkr, ShotK47, ShotM4c, ShotFm2,
    ShotBpa, ShotGl4, ShotDmr20, ShotSvd, ShotScout, ShotAwm, ShotM82, ShotPump12, ShotAuto12, ShotSawn,
    ShotPara, ShotPk, ShotRpg, ShotThumper,
    // Reloading and handling.
    MagOutLight, MagInLight, MagOutHeavy, MagInHeavy, BoltPull, BoltCycle, SlideRelease, ShellInsert,
    PumpRack, RocketLoad, BeltLoad, RevolverLoad, BreakAction, EmptyClick, WeaponDraw, AdsIn, AdsOut,
    ScopeZoom,
    // Movement.
    StepConcreteL, StepConcreteR, StepMetalL, StepMetalR, StepGravelL, StepGravelR, StepGrassL, StepGrassR,
    StepWoodL, StepWoodR, StepWaterL, StepWaterR, WaterSplash, Jump, Land,
    // Bullet impacts and hits.
    ImpactConcrete, ImpactMetal, ImpactWood, ImpactGlass, ImpactDirt, ImpactFlesh, HelmetClank,
    HitConfirm, HitConfirmHead, HurtThud, BodyFall, BulletWhiz,
    // Grenades, explosions, fire.
    PinPull, SpoonPing, Throw, GrenadeBounce, Explosion, ExplosionDistant, FlashBang, FlashRing, SmokePop,
    SmokeHiss, IncenIgnite, FireCrackle, RocketFly,
    // Melee.
    KnifeSwing, HeavySwing, KnifeHit, AxeHit, CrowbarHit, MacheteHit,
    // Pickups, interface, match flow.
    PickupWeapon, PickupAmmo, MenuHover, MenuClick, MenuConfirm, MenuBack, KillcamWhoosh, MatchStart,
    MatchEnd, Respawn,
    // One-shot ambience, three takes each.
    AmbClang, AmbCreak, AmbDrip, AmbSteam, AmbBird, AmbPigeon,
}

/// The sound index of `sfx` in the `Rendered` bank.
pub fn index(sfx: Sfx) -> usize {
    sfx as usize
}

/// True when the cue has [`PAN_STEPS`] stereo pan variants.
pub fn is_directional(sfx: Sfx) -> bool {
    use Sfx::*;
    let i = sfx as usize;
    if i <= ShotThumper as usize {
        return true;
    }
    matches!(
        sfx,
        MagOutLight
            | MagInLight
            | MagOutHeavy
            | MagInHeavy
            | BoltPull
            | BoltCycle
            | SlideRelease
            | ShellInsert
            | PumpRack
            | RocketLoad
            | BeltLoad
            | RevolverLoad
            | BreakAction
            | StepConcreteL
            | StepConcreteR
            | StepMetalL
            | StepMetalR
            | StepGravelL
            | StepGravelR
            | StepGrassL
            | StepGrassR
            | StepWoodL
            | StepWoodR
            | StepWaterL
            | StepWaterR
            | WaterSplash
            | Jump
            | Land
            | ImpactConcrete
            | ImpactMetal
            | ImpactWood
            | ImpactGlass
            | ImpactDirt
            | ImpactFlesh
            | HelmetClank
            | BodyFall
            | BulletWhiz
            | Throw
            | GrenadeBounce
            | Explosion
            | ExplosionDistant
            | FlashBang
            | SmokePop
            | SmokeHiss
            | IncenIgnite
            | FireCrackle
            | RocketFly
            | KnifeSwing
            | HeavySwing
            | KnifeHit
            | AxeHit
            | CrowbarHit
            | MacheteHit
    )
}

/// Number of variants the bank holds for `sfx`.
pub fn variant_count(sfx: Sfx) -> usize {
    if is_directional(sfx) {
        PAN_STEPS
    } else if (Sfx::AmbClang as usize..=Sfx::AmbPigeon as usize).contains(&(sfx as usize)) {
        AMB_TAKES
    } else {
        1
    }
}

/// Which variant to play for a pan in `-1.0` (hard left) ..= `1.0` (hard right).
pub fn pan_variant(pan: f32) -> usize {
    let p = if pan.is_finite() { pan.clamp(-1., 1.) } else { 0. };
    (((p + 1.) * 0.5 * (PAN_STEPS - 1) as f32).round() as usize).min(PAN_STEPS - 1)
}

/// The pan (-1..1) that variant `v` was rendered at.
pub fn variant_pan(v: usize) -> f32 {
    -1. + 2. * v.min(PAN_STEPS - 1) as f32 / (PAN_STEPS - 1) as f32
}

/// Ground surfaces for footsteps and impacts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Concrete,
    Metal,
    Gravel,
    Grass,
    Wood,
    Water,
    Glass,
    Dirt,
    Flesh,
}

/// Footstep cue for a surface and foot (glass and flesh fall back to concrete, dirt to grass).
pub fn step(surface: Surface, left: bool) -> Sfx {
    use Sfx::*;
    let (l, r) = match surface {
        Surface::Metal => (StepMetalL, StepMetalR),
        Surface::Gravel => (StepGravelL, StepGravelR),
        Surface::Grass | Surface::Dirt => (StepGrassL, StepGrassR),
        Surface::Wood => (StepWoodL, StepWoodR),
        Surface::Water => (StepWaterL, StepWaterR),
        Surface::Concrete | Surface::Glass | Surface::Flesh => (StepConcreteL, StepConcreteR),
    };
    if left {
        l
    } else {
        r
    }
}

/// Bullet impact cue for a surface (gravel, grass and water use the dirt puff).
pub fn impact(surface: Surface) -> Sfx {
    match surface {
        Surface::Concrete => Sfx::ImpactConcrete,
        Surface::Metal => Sfx::ImpactMetal,
        Surface::Wood => Sfx::ImpactWood,
        Surface::Glass => Sfx::ImpactGlass,
        Surface::Flesh => Sfx::ImpactFlesh,
        Surface::Gravel | Surface::Grass | Surface::Dirt | Surface::Water => Sfx::ImpactDirt,
    }
}

/// The gunshot cue for a roster key (`"k47"`), `None` for grenades, melee and unknown keys.
pub fn shot_for_key(key: &str) -> Option<Sfx> {
    use Sfx::*;
    Some(match key {
        "k9" => ShotK9,
        "m45" => ShotM45,
        "hc50" => ShotHc50,
        "rv357" => ShotRv357,
        "mp9" => ShotMp9,
        "ump" => ShotUmp,
        "pdw" => ShotPdw,
        "vkr" => ShotVkr,
        "k47" => ShotK47,
        "m4c" => ShotM4c,
        "fm2" => ShotFm2,
        "bpa" => ShotBpa,
        "gl4" => ShotGl4,
        "dmr20" => ShotDmr20,
        "svd" => ShotSvd,
        "scout" => ShotScout,
        "awm" => ShotAwm,
        "m82" => ShotM82,
        "pump12" => ShotPump12,
        "auto12" => ShotAuto12,
        "sawn" => ShotSawn,
        "para" => ShotPara,
        "pk" => ShotPk,
        "rpg" => ShotRpg,
        "thumper" => ShotThumper,
        _ => return None,
    })
}

/// The melee hit cue for a roster key (`knife`, `machete`, `axe`, `crowbar`).
pub fn melee_hit_for_key(key: &str) -> Option<Sfx> {
    Some(match key {
        "knife" => Sfx::KnifeHit,
        "machete" => Sfx::MacheteHit,
        "axe" => Sfx::AxeHit,
        "crowbar" => Sfx::CrowbarHit,
        _ => return None,
    })
}

/// The reload sounds of a weapon as `(fraction of the reload time, cue)`; play each when the reload
/// timer passes its fraction. For shotguns with per-shell reload (`pump12`, `auto12`) the sequence is one
/// shell insert per shell; play [`Sfx::PumpRack`] after a pump-action shot. Empty for grenades and melee.
pub fn reload_sequence(key: &str) -> &'static [(f32, Sfx)] {
    use Sfx::*;
    match key {
        "k9" | "m45" | "hc50" => &[(0.0, MagOutLight), (0.55, MagInLight), (0.85, SlideRelease)],
        "mp9" | "ump" | "pdw" | "vkr" => &[(0.0, MagOutLight), (0.55, MagInLight), (0.85, BoltPull)],
        "k47" | "m4c" | "fm2" | "bpa" | "gl4" | "dmr20" | "svd" => {
            &[(0.0, MagOutHeavy), (0.5, MagInHeavy), (0.82, BoltPull)]
        }
        "awm" | "m82" => &[(0.0, MagOutHeavy), (0.5, MagInHeavy), (0.85, BoltCycle)],
        "scout" => &[(0.0, ShellInsert), (0.25, ShellInsert), (0.5, ShellInsert), (0.8, BoltCycle)],
        "rv357" => &[(0.0, RevolverLoad)],
        "pump12" | "auto12" => &[(0.0, ShellInsert)],
        "sawn" | "thumper" => &[(0.0, BreakAction)],
        "para" | "pk" => &[(0.0, BeltLoad), (0.75, BoltPull)],
        "rpg" => &[(0.0, RocketLoad)],
        _ => &[],
    }
}

// ------------------------------------------------------------------------------------------ DSP kit

fn n(sec: f32) -> usize {
    (sec.max(0.) * SR).round() as usize
}

fn gen(dur: f32, mut f: impl FnMut(f32) -> f32) -> Vec<f32> {
    (0..n(dur)).map(|i| f(i as f32 / SR)).collect()
}

fn mul(a: &mut [f32], f: impl Fn(f32) -> f32) {
    for (i, x) in a.iter_mut().enumerate() {
        *x *= f(i as f32 / SR);
    }
}

/// Linear attack `a`, then exponential decay with time constant `tau`.
fn ad(t: f32, a: f32, tau: f32) -> f32 {
    if t < 0. {
        0.
    } else {
        (t / a.max(1e-5)).min(1.) * (-t / tau.max(1e-5)).exp()
    }
}

fn noise(rng: &mut Rng, count: usize) -> Vec<f32> {
    (0..count).map(|_| rng.f32() * 2. - 1.).collect()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0f32, |m, v| m.max(v.abs()))
}

fn scale_to_peak(x: &mut [f32], target: f32) {
    let p = peak(x);
    if p > 1e-9 {
        let g = target / p;
        x.iter_mut().for_each(|v| *v *= g);
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Low,
    High,
    Band,
}

/// RBJ biquad in f64 (low cutoffs stay accurate).
#[derive(Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn new(kind: Kind, fc: f32, q: f32) -> Self {
        let mut b = Self { b0: 0., b1: 0., b2: 0., a1: 0., a2: 0., x1: 0., x2: 0., y1: 0., y2: 0. };
        b.set(kind, fc, q);
        b
    }
    fn set(&mut self, kind: Kind, fc: f32, q: f32) {
        let fc = (fc as f64).clamp(20., 0.45 * SR as f64);
        let w = std::f64::consts::TAU * fc / SR as f64;
        let (s, c) = w.sin_cos();
        let alpha = s / (2. * (q as f64).max(0.1));
        let (b0, b1, b2) = match kind {
            Kind::Low => ((1. - c) / 2., 1. - c, (1. - c) / 2.),
            Kind::High => ((1. + c) / 2., -(1. + c), (1. + c) / 2.),
            Kind::Band => (alpha, 0., -alpha),
        };
        let a0 = 1. + alpha;
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = -2. * c / a0;
        self.a2 = (1. - alpha) / a0;
    }
    fn process(&mut self, x: f32) -> f32 {
        let x = x as f64;
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y as f32
    }
}

fn filt(x: &[f32], kind: Kind, fc: f32, q: f32) -> Vec<f32> {
    let mut b = Biquad::new(kind, fc, q);
    x.iter().map(|&v| b.process(v)).collect()
}

fn lp(x: &[f32], fc: f32) -> Vec<f32> {
    filt(x, Kind::Low, fc, 0.707)
}

fn hp(x: &[f32], fc: f32) -> Vec<f32> {
    filt(x, Kind::High, fc, 0.707)
}

fn bp(x: &[f32], fc: f32, q: f32) -> Vec<f32> {
    filt(x, Kind::Band, fc, q)
}

/// Filter with a time-varying cutoff (`fc(t)` in Hz, updated every 16 samples).
fn filt_var(x: &[f32], kind: Kind, fc: impl Fn(f32) -> f32, q: f32) -> Vec<f32> {
    let mut b = Biquad::new(kind, fc(0.), q);
    let mut out = Vec::with_capacity(x.len());
    for (i, &v) in x.iter().enumerate() {
        if i % 16 == 0 {
            b.set(kind, fc(i as f32 / SR), q);
        }
        out.push(b.process(v));
    }
    out
}

/// Band-limited noise burst, normalised to peak 1 before the `tau` decay envelope.
fn burst(rng: &mut Rng, dur: f32, hp_f: f32, lp_f: f32, tau: f32) -> Vec<f32> {
    let mut x = noise(rng, n(dur));
    if hp_f > 0. {
        x = hp(&x, hp_f);
    }
    if lp_f < 10_000. {
        x = lp(&x, lp_f);
    }
    scale_to_peak(&mut x, 1.);
    mul(&mut x, |t| ad(t, 0.0003, tau));
    x
}

/// A sine whose frequency decays from `f0` to `f1` (`ftau`), amplitude decay `atau`, `drive` of 2nd harmonic.
fn sweep(dur: f32, f0: f32, f1: f32, ftau: f32, atau: f32, drive: f32) -> Vec<f32> {
    let mut ph = 0f64;
    gen(dur, |t| {
        let f = f1 + (f0 - f1) * (-t / ftau.max(1e-4)).exp();
        ph += std::f64::consts::TAU * f as f64 / SR as f64;
        let s = ph.sin() as f32 + drive * (2. * ph).sin() as f32;
        s * ad(t, 0.0008, atau)
    })
}

/// A sum of decaying sines `(frequency, amplitude, tau)`: struck metal and wood.
fn modal(dur: f32, parts: &[(f32, f32, f32)]) -> Vec<f32> {
    let mut out = vec![0f32; n(dur)];
    for &(f, a, tau) in parts {
        let w = TAU * f / SR;
        for (i, o) in out.iter_mut().enumerate() {
            let t = i as f32 / SR;
            *o += a * (w * i as f32).sin() * ad(t, 0.0004, tau);
        }
    }
    out
}

/// Whoosh: band-passed noise whose centre sweeps `f0 -> f1` over the clip, with a smooth hump envelope.
fn whoosh(rng: &mut Rng, dur: f32, f0: f32, f1: f32, q: f32, skew: f32) -> Vec<f32> {
    let x = noise(rng, n(dur));
    let mut y = filt_var(&x, Kind::Band, |t| f0 + (f1 - f0) * (t / dur).min(1.), q);
    scale_to_peak(&mut y, 1.);
    mul(&mut y, |t| {
        let k = (t / dur).clamp(0., 1.);
        (PI * k.powf(skew)).sin().max(0.)
    });
    y
}

struct Mix(Vec<f32>);

impl Mix {
    fn new(dur: f32) -> Self {
        Mix(vec![0.; n(dur)])
    }
    fn add(&mut self, at: f32, src: &[f32], g: f32) {
        let s = n(at);
        if s >= self.0.len() {
            return;
        }
        for (d, v) in self.0[s..].iter_mut().zip(src) {
            *d += v * g;
        }
    }
}

fn soft_clip(x: &mut [f32], drive: f32) {
    let norm = drive.tanh();
    for v in x.iter_mut() {
        *v = (*v * drive).tanh() / norm;
    }
}

/// Small Schroeder reverb: five damped combs and two all-pass stages; the output is longer than the input.
fn reverb(x: &[f32], rt60: f32, wet: f32) -> Vec<f32> {
    let len = x.len() + n(rt60 * 0.9);
    let mut inp = x.to_vec();
    inp.resize(len, 0.);
    let mut acc = vec![0f32; len];
    for d in [0.0297f32, 0.0371, 0.0411, 0.0437, 0.0533] {
        let dl = n(d).max(1);
        let fb = 10f32.powf(-3. * d / rt60.max(0.05));
        let mut buf = vec![0f32; dl];
        let (mut idx, mut z) = (0usize, 0f32);
        for i in 0..len {
            let y = buf[idx];
            z = 0.5 * y + 0.5 * z;
            buf[idx] = inp[i] + fb * z;
            idx = (idx + 1) % dl;
            acc[i] += y;
        }
    }
    for (d, g) in [(0.005f32, 0.5f32), (0.0017, 0.5)] {
        let dl = n(d).max(1);
        let mut buf = vec![0f32; dl];
        let mut idx = 0usize;
        for v in acc.iter_mut() {
            let b = buf[idx];
            let w = *v + g * b;
            buf[idx] = w;
            *v = b - g * w;
            idx = (idx + 1) % dl;
        }
    }
    inp.iter().zip(&acc).map(|(d, r)| d + wet * 0.4 * r).collect()
}

/// Discrete echoes `(delay s, gain, low-pass Hz)` off far walls; the output is longer than the input.
fn echoes(x: &[f32], taps: &[(f32, f32, f32)]) -> Vec<f32> {
    let maxd = taps.iter().fold(0f32, |m, t| m.max(t.0));
    let mut m = Mix::new(x.len() as f32 / SR + maxd);
    m.add(0., x, 1.);
    for &(d, g, fc) in taps {
        m.add(d, &lp(x, fc), g);
    }
    m.0
}

/// Scrub, fade the first 2 ms and the last `fade_out_ms`, then scale to exactly `target` peak.
fn finish(mut v: Vec<f32>, target: f32, fade_out_ms: f32) -> Vec<f32> {
    for x in &mut v {
        if !x.is_finite() {
            *x = 0.;
        }
    }
    let len = v.len();
    let fi = n(0.002).min(len);
    for (i, x) in v.iter_mut().take(fi).enumerate() {
        *x *= 0.5 - 0.5 * (PI * i as f32 / fi as f32).cos();
    }
    let fo = n(fade_out_ms / 1000.).min(len);
    for (i, x) in v.iter_mut().rev().take(fo).enumerate() {
        *x *= 0.5 - 0.5 * (PI * i as f32 / fo as f32).cos();
    }
    scale_to_peak(&mut v, target);
    v
}

/// Render mono `m` at `pan` (-1..1) as a stereo WAV: the far ear is quieter (never below 0.1), delayed up
/// to 12 samples (0.54 ms) and darkened; the near ear keeps the original. Centre is identical in both.
fn pan_wav(m: &[f32], pan: f32) -> Vec<u8> {
    let th = (pan + 1.) * FRAC_PI_4; // 0 (left) .. pi/2 (right)
    let gl = 0.1 + 0.9 * (2f32.sqrt() * th.cos()).min(1.);
    let gr = 0.1 + 0.9 * (2f32.sqrt() * th.sin()).min(1.);
    let a = pan.abs();
    let delay = (a * 12.).round() as usize;
    let k = 1. - 0.6 * a;
    let mut far = Vec::with_capacity(m.len() + delay);
    far.resize(delay, 0.);
    let mut z = 0f32;
    for &v in m {
        z += k * (v - z);
        far.push(z);
    }
    let mut near = m.to_vec();
    near.resize(far.len(), 0.);
    let (near_g, far_g, near_is_left) = if pan < 0. { (gl, gr, true) } else { (gr, gl, false) };
    let near: Vec<f32> = near.iter().map(|v| v * near_g).collect();
    let far: Vec<f32> = far.iter().map(|v| v * far_g).collect();
    let (l, r) = if near_is_left { (near, far) } else { (far, near) };
    wav_bytes_stereo(&l, &r, RATE)
}

// ------------------------------------------------------------------------------------------ analysis

/// Measurements of a clip, for tests and the dump tool.
#[derive(Clone, Copy, Debug)]
pub struct Stats {
    pub peak: f32,
    pub rms: f32,
    /// Power-weighted mean frequency of the first ~3 s, Hz.
    pub centroid: f32,
    pub seconds: f32,
    /// Time until the 5 ms RMS envelope has fallen 30 dB below its maximum for the last time, seconds.
    pub decay: f32,
}

fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -std::f64::consts::TAU / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1f64, 0f64);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

/// Spectral centroid in Hz of the first 65536 samples (Hann window), 0 for silence.
pub fn spectral_centroid(x: &[f32]) -> f32 {
    let m = x.len().min(65_536);
    if m < 16 {
        return 0.;
    }
    let size = m.next_power_of_two();
    let mut re = vec![0f64; size];
    let mut im = vec![0f64; size];
    for i in 0..m {
        let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / m as f64).cos();
        re[i] = x[i] as f64 * w;
    }
    fft(&mut re, &mut im);
    let (mut num, mut den) = (0f64, 0f64);
    for k in 1..size / 2 {
        let p = re[k] * re[k] + im[k] * im[k];
        num += p * k as f64 * RATE as f64 / size as f64;
        den += p;
    }
    if den > 0. {
        (num / den) as f32
    } else {
        0.
    }
}

/// Decay time, see [`Stats::decay`].
pub fn decay_time(x: &[f32]) -> f32 {
    let w = 110usize;
    let env: Vec<f32> = x.chunks(w).map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt()).collect();
    let top = env.iter().fold(0f32, |m, v| m.max(*v));
    if top <= 0. {
        return 0.;
    }
    let last = env.iter().rposition(|&e| e > top * 0.0316).unwrap_or(0);
    (last + 1) as f32 * w as f32 / SR
}

/// Measure mono samples.
pub fn analyse(x: &[f32]) -> Stats {
    Stats {
        peak: peak(x),
        rms: (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt(),
        centroid: spectral_centroid(x),
        seconds: x.len() as f32 / SR,
        decay: decay_time(x),
    }
}

// ------------------------------------------------------------------------------------------ guns

/// Parameters of a firearm: crack (noise burst), body (low-passed noise), boom (falling sine), brass
/// tinkle and the room / echo tail.
#[derive(Clone, Copy)]
struct Gun {
    dur: f32,
    chp: f32,
    clp: f32,
    ctau: f32,
    cg: f32,
    f0: f32,
    f1: f32,
    ftau: f32,
    btau: f32,
    bg: f32,
    nlp: f32,
    ntau: f32,
    ng: f32,
    brass: f32,
    brass_hz: f32,
    rt60: f32,
    wet: f32,
    echo: &'static [(f32, f32, f32)],
}

const GUN: Gun = Gun {
    dur: 0.4,
    chp: 800.,
    clp: 6000.,
    ctau: 0.012,
    cg: 1.,
    f0: 140.,
    f1: 80.,
    ftau: 0.02,
    btau: 0.05,
    bg: 0.7,
    nlp: 1200.,
    ntau: 0.05,
    ng: 0.5,
    brass: 0.,
    brass_hz: 5200.,
    rt60: 0.3,
    wet: 0.3,
    echo: &[],
};

fn gun_params(s: Sfx) -> Gun {
    use Sfx::*;
    let g = GUN;
    match s {
        ShotK9 => Gun {
            dur: 0.3,
            chp: 1200.,
            clp: 7000.,
            ctau: 0.009,
            f0: 200.,
            f1: 110.,
            btau: 0.035,
            bg: 0.2,
            nlp: 1800.,
            ntau: 0.03,
            ng: 0.4,
            brass: 0.06,
            rt60: 0.25,
            wet: 0.25,
            ..g
        },
        ShotM45 => Gun {
            dur: 0.35,
            chp: 900.,
            clp: 5500.,
            ctau: 0.012,
            f0: 150.,
            f1: 85.,
            btau: 0.06,
            bg: 0.3,
            nlp: 1400.,
            ntau: 0.045,
            brass: 0.07,
            brass_hz: 4400.,
            rt60: 0.3,
            ..g
        },
        ShotHc50 => Gun {
            dur: 0.6,
            chp: 500.,
            clp: 5000.,
            ctau: 0.02,
            f0: 110.,
            f1: 50.,
            ftau: 0.04,
            btau: 0.14,
            bg: 0.7,
            nlp: 900.,
            ntau: 0.1,
            ng: 0.7,
            brass: 0.08,
            brass_hz: 3600.,
            rt60: 0.7,
            wet: 0.4,
            echo: &[(0.11, 0.25, 2500.)],
            ..g
        },
        ShotRv357 => Gun {
            dur: 0.45,
            chp: 700.,
            clp: 5500.,
            ctau: 0.016,
            f0: 130.,
            f1: 65.,
            btau: 0.09,
            bg: 0.45,
            nlp: 1100.,
            ntau: 0.07,
            ng: 0.6,
            rt60: 0.5,
            wet: 0.35,
            ..g
        },
        ShotMp9 => Gun {
            dur: 0.25,
            chp: 1500.,
            clp: 7500.,
            ctau: 0.008,
            f0: 170.,
            f1: 100.,
            btau: 0.035,
            bg: 0.18,
            nlp: 2000.,
            ntau: 0.03,
            ng: 0.4,
            brass: 0.05,
            rt60: 0.25,
            wet: 0.25,
            ..g
        },
        ShotUmp => Gun {
            dur: 0.28,
            chp: 1000.,
            clp: 6000.,
            ctau: 0.013,
            f0: 130.,
            f1: 75.,
            btau: 0.055,
            bg: 0.28,
            nlp: 1500.,
            ntau: 0.04,
            brass: 0.05,
            brass_hz: 4300.,
            ..g
        },
        ShotPdw => Gun {
            dur: 0.2,
            chp: 2800.,
            clp: 9500.,
            ctau: 0.005,
            f0: 260.,
            f1: 170.,
            btau: 0.02,
            bg: 0.1,
            nlp: 3000.,
            ntau: 0.015,
            ng: 0.25,
            brass: 0.04,
            brass_hz: 7000.,
            rt60: 0.2,
            wet: 0.2,
            ..g
        },
        ShotVkr => Gun {
            dur: 0.22,
            chp: 1600.,
            clp: 8000.,
            ctau: 0.007,
            f0: 150.,
            f1: 95.,
            btau: 0.03,
            bg: 0.15,
            nlp: 2200.,
            ntau: 0.025,
            ng: 0.35,
            brass: 0.04,
            brass_hz: 4400.,
            rt60: 0.22,
            wet: 0.25,
            ..g
        },
        ShotK47 => Gun {
            dur: 0.65,
            chp: 600.,
            clp: 6000.,
            ctau: 0.02,
            f0: 110.,
            f1: 58.,
            ftau: 0.03,
            btau: 0.1,
            bg: 0.6,
            nlp: 1000.,
            ntau: 0.08,
            ng: 0.7,
            brass: 0.06,
            brass_hz: 3900.,
            rt60: 0.8,
            wet: 0.45,
            echo: &[(0.13, 0.2, 2000.)],
            ..g
        },
        ShotM4c => Gun {
            dur: 0.6,
            chp: 1100.,
            clp: 7000.,
            ctau: 0.015,
            f0: 130.,
            f1: 72.,
            btau: 0.07,
            bg: 0.35,
            nlp: 1500.,
            ntau: 0.055,
            ng: 0.55,
            brass: 0.06,
            brass_hz: 5000.,
            rt60: 0.7,
            wet: 0.4,
            ..g
        },
        ShotFm2 => Gun {
            dur: 0.5,
            chp: 1300.,
            clp: 7500.,
            ctau: 0.012,
            f0: 150.,
            f1: 80.,
            btau: 0.05,
            bg: 0.3,
            nlp: 1700.,
            ntau: 0.045,
            brass: 0.05,
            rt60: 0.6,
            wet: 0.35,
            ..g
        },
        ShotBpa => Gun {
            dur: 0.55,
            chp: 800.,
            clp: 4500.,
            ctau: 0.016,
            f0: 120.,
            f1: 68.,
            btau: 0.07,
            bg: 0.5,
            nlp: 1200.,
            ntau: 0.06,
            ng: 0.6,
            rt60: 0.65,
            wet: 0.4,
            ..g
        },
        ShotGl4 => Gun {
            dur: 0.85,
            chp: 450.,
            clp: 5500.,
            ctau: 0.025,
            f0: 95.,
            f1: 48.,
            ftau: 0.035,
            btau: 0.14,
            bg: 1.,
            nlp: 800.,
            ntau: 0.1,
            ng: 0.75,
            rt60: 0.9,
            wet: 0.5,
            echo: &[(0.15, 0.3, 1800.)],
            ..g
        },
        ShotDmr20 => Gun {
            dur: 0.9,
            chp: 700.,
            clp: 6500.,
            ctau: 0.02,
            f0: 100.,
            f1: 52.,
            btau: 0.12,
            bg: 0.9,
            nlp: 900.,
            ntau: 0.09,
            ng: 0.65,
            rt60: 1.,
            wet: 0.5,
            echo: &[(0.17, 0.3, 2200.), (0.35, 0.15, 1500.)],
            ..g
        },
        ShotSvd => Gun {
            dur: 0.9,
            chp: 900.,
            clp: 7500.,
            ctau: 0.017,
            cg: 1.1,
            f0: 105.,
            f1: 55.,
            btau: 0.1,
            bg: 0.8,
            nlp: 1100.,
            ntau: 0.08,
            ng: 0.6,
            rt60: 1.,
            wet: 0.5,
            echo: &[(0.2, 0.35, 2500.), (0.42, 0.18, 1800.)],
            ..g
        },
        ShotScout => Gun {
            dur: 1.3,
            chp: 900.,
            clp: 7000.,
            ctau: 0.02,
            f0: 100.,
            f1: 55.,
            btau: 0.1,
            bg: 0.8,
            nlp: 1000.,
            ntau: 0.08,
            ng: 0.6,
            rt60: 1.4,
            wet: 0.55,
            echo: &[(0.25, 0.4, 2500.), (0.55, 0.25, 1800.), (0.95, 0.12, 1200.)],
            ..g
        },
        ShotAwm => Gun {
            dur: 1.6,
            chp: 500.,
            clp: 6500.,
            ctau: 0.03,
            f0: 80.,
            f1: 38.,
            ftau: 0.06,
            btau: 0.22,
            bg: 1.,
            nlp: 700.,
            ntau: 0.15,
            ng: 0.85,
            rt60: 1.7,
            wet: 0.6,
            echo: &[(0.3, 0.45, 2000.), (0.65, 0.3, 1500.), (1.1, 0.18, 1000.)],
            ..g
        },
        ShotM82 => Gun {
            dur: 1.8,
            chp: 350.,
            clp: 5500.,
            ctau: 0.04,
            cg: 0.9,
            f0: 62.,
            f1: 28.,
            ftau: 0.09,
            btau: 0.4,
            bg: 1.2,
            nlp: 500.,
            ntau: 0.22,
            ng: 1.,
            rt60: 1.9,
            wet: 0.6,
            echo: &[(0.35, 0.5, 1500.), (0.75, 0.35, 1200.), (1.3, 0.2, 900.)],
            ..g
        },
        ShotPump12 => Gun {
            dur: 0.7,
            chp: 400.,
            clp: 4500.,
            ctau: 0.03,
            f0: 100.,
            f1: 52.,
            btau: 0.13,
            bg: 0.9,
            nlp: 1500.,
            ntau: 0.08,
            ng: 1.,
            rt60: 0.7,
            wet: 0.4,
            echo: &[(0.1, 0.2, 2500.)],
            ..g
        },
        ShotAuto12 => Gun {
            dur: 0.6,
            chp: 600.,
            clp: 5000.,
            ctau: 0.022,
            f0: 110.,
            f1: 58.,
            btau: 0.1,
            bg: 0.8,
            nlp: 1800.,
            ntau: 0.065,
            ng: 1.,
            rt60: 0.55,
            ..g
        },
        ShotSawn => Gun {
            dur: 0.8,
            chp: 350.,
            clp: 4500.,
            ctau: 0.04,
            f0: 90.,
            f1: 40.,
            ftau: 0.05,
            btau: 0.2,
            bg: 1.1,
            nlp: 1100.,
            ntau: 0.12,
            ng: 1.1,
            rt60: 0.9,
            wet: 0.5,
            echo: &[(0.14, 0.3, 2000.)],
            ..g
        },
        ShotPara => Gun {
            dur: 0.4,
            chp: 700.,
            clp: 6500.,
            ctau: 0.014,
            f0: 115.,
            f1: 66.,
            btau: 0.06,
            bg: 0.4,
            nlp: 1300.,
            ntau: 0.05,
            rt60: 0.45,
            ..g
        },
        ShotPk => Gun {
            dur: 0.5,
            chp: 450.,
            clp: 5500.,
            ctau: 0.02,
            f0: 90.,
            f1: 50.,
            btau: 0.1,
            bg: 1.,
            nlp: 850.,
            ntau: 0.08,
            ng: 0.65,
            rt60: 0.6,
            wet: 0.4,
            ..g
        },
        _ => g,
    }
}

fn gun(rng: &mut Rng, g: &Gun) -> Vec<f32> {
    let mut m = Mix::new(g.dur.min(0.6));
    m.add(0., &burst(rng, 0.3, g.chp, g.clp, g.ctau), g.cg);
    m.add(0., &burst(rng, 0.6, 0., g.nlp, g.ntau), g.ng);
    m.add(0., &sweep(g.dur, g.f0, g.f1, g.ftau, g.btau, 0.35), g.bg);
    if g.brass > 0. {
        let at = rng.range(0.22, 0.3).min(g.dur - 0.1);
        let f = g.brass_hz;
        let b = modal(0.12, &[(f, 1., 0.03), (f * 1.52, 0.6, 0.02), (f * 2.1, 0.3, 0.015)]);
        m.add(at, &b, g.brass);
    }
    let mut x = m.0;
    soft_clip(&mut x, 1.6);
    let mut x = reverb(&x, g.rt60, g.wet);
    if !g.echo.is_empty() {
        x = echoes(&x, g.echo);
    }
    x
}

fn rocket_launch(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(2.0);
    m.add(0., &sweep(0.6, 95., 35., 0.12, 0.15, 0.3), 1.);
    m.add(0., &burst(rng, 0.3, 300., 4000., 0.04), 0.8);
    let mut w = whoosh(rng, 1.7, 500., 2400., 0.9, 0.55);
    mul(&mut w, |t| ad(t, 0.05, 0.9));
    m.add(0.03, &w, 0.9);
    let mut motor = lp(&noise(rng, n(1.8)), 500.);
    scale_to_peak(&mut motor, 1.);
    mul(&mut motor, |t| ad(t, 0.05, 0.6));
    m.add(0.03, &motor, 0.6);
    let mut x = m.0;
    soft_clip(&mut x, 1.4);
    reverb(&x, 1.0, 0.35)
}

fn thumper_shot(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.6);
    m.add(0., &sweep(0.5, 130., 58., 0.03, 0.09, 0.3), 1.);
    m.add(0., &sweep(0.3, 220., 120., 0.05, 0.1, 0.), 0.45);
    m.add(0., &burst(rng, 0.3, 500., 3000., 0.025), 0.45);
    m.add(0., &burst(rng, 0.4, 0., 800., 0.06), 0.5);
    let mut x = m.0;
    soft_clip(&mut x, 1.4);
    reverb(&x, 0.5, 0.35)
}

fn rocket_fly(rng: &mut Rng) -> Vec<f32> {
    let mut w = whoosh(rng, 0.6, 700., 1400., 0.8, 1.0);
    mul(&mut w, |t| 0.6 + 0.4 * (TAU * 38. * t).sin());
    w
}

fn bullet_whiz(rng: &mut Rng) -> Vec<f32> {
    let x = noise(rng, n(0.22));
    let mut y = filt_var(&x, Kind::Band, |t| 4800. - 3600. * (t / 0.22), 3.);
    scale_to_peak(&mut y, 1.);
    mul(&mut y, |t| (PI * (t / 0.22).min(1.)).sin().powf(1.5));
    y
}

// ------------------------------------------------------------------------------------------ handling

/// A short mechanical click: noise tick plus a metallic ring at `fc`.
fn clack(rng: &mut Rng, fc: f32, tau: f32, amp: f32) -> Vec<f32> {
    let d = (tau * 8.).max(0.03);
    let mut x = burst(rng, d, fc * 0.6, fc * 2.5, tau);
    let ring = modal(d, &[(fc, 0.5, tau * 1.5), (fc * 2.33, 0.25, tau)]);
    for (a, b) in x.iter_mut().zip(&ring) {
        *a = *a * 0.7 + b;
    }
    x.iter_mut().for_each(|v| *v *= amp);
    x
}

fn thump(f: f32, tau: f32, amp: f32) -> Vec<f32> {
    let mut x = sweep(tau * 6., f * 1.5, f, 0.02, tau, 0.);
    x.iter_mut().for_each(|v| *v *= amp);
    x
}

/// Sliding metal friction.
fn rasp(rng: &mut Rng, dur: f32, f0: f32, f1: f32, amp: f32) -> Vec<f32> {
    let mut w = whoosh(rng, dur, f0, f1, 1.6, 1.0);
    w.iter_mut().for_each(|v| *v *= amp);
    w
}

fn mag_out(rng: &mut Rng, heavy: bool) -> Vec<f32> {
    let (fc, f) = if heavy { (1100., 90.) } else { (2200., 140.) };
    let mut m = Mix::new(0.4);
    m.add(0., &clack(rng, fc * 1.3, 0.008, 0.9), 1.);
    m.add(0.03, &rasp(rng, 0.11, fc * 0.5, fc * 0.8, 0.5), 1.);
    m.add(0.17, &thump(f, 0.03, 0.6), 1.);
    m.add(0.17, &clack(rng, fc * 0.7, 0.01, 0.25), 1.);
    m.0
}

fn mag_in(rng: &mut Rng, heavy: bool) -> Vec<f32> {
    let (fc, f) = if heavy { (1000., 80.) } else { (2000., 130.) };
    let mut m = Mix::new(0.4);
    m.add(0., &rasp(rng, 0.1, fc * 0.6, fc * 0.4, 0.45), 1.);
    m.add(0.11, &clack(rng, fc, 0.012, 1.), 1.);
    m.add(0.11, &thump(f, 0.04, 0.8), 1.);
    m.add(0.2, &clack(rng, fc * 1.4, 0.006, 0.35), 1.);
    m.0
}

fn bolt_pull(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.45);
    m.add(0., &rasp(rng, 0.13, 700., 1300., 0.6), 1.);
    m.add(0.13, &clack(rng, 1800., 0.01, 0.7), 1.);
    m.add(0.25, &clack(rng, 1300., 0.014, 1.), 1.);
    m.add(0.25, &thump(100., 0.03, 0.4), 1.);
    m.0
}

fn bolt_cycle(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.85);
    m.add(0., &clack(rng, 1500., 0.01, 0.6), 1.);
    m.add(0.05, &rasp(rng, 0.17, 600., 1100., 0.5), 1.);
    m.add(0.22, &clack(rng, 2000., 0.008, 0.6), 1.);
    m.add(0.42, &rasp(rng, 0.14, 1000., 600., 0.5), 1.);
    m.add(0.56, &clack(rng, 1200., 0.014, 1.), 1.);
    m.add(0.56, &thump(90., 0.04, 0.5), 1.);
    m.add(0.68, &clack(rng, 1800., 0.008, 0.45), 1.);
    m.0
}

fn slide_release(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.2);
    m.add(0., &clack(rng, 2600., 0.006, 1.), 1.);
    m.add(0.012, &rasp(rng, 0.05, 2500., 1800., 0.4), 1.);
    m.add(0.065, &clack(rng, 1800., 0.008, 0.7), 1.);
    m.0
}

fn shell_insert(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.22);
    m.add(0., &clack(rng, 1500., 0.008, 0.8), 1.);
    m.add(0.03, &thump(180., 0.02, 0.5), 1.);
    m.add(0.09, &clack(rng, 900., 0.012, 0.7), 1.);
    m.0
}

fn pump_rack(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.42);
    m.add(0., &clack(rng, 1000., 0.016, 1.), 1.);
    m.add(0., &rasp(rng, 0.08, 500., 900., 0.4), 1.);
    m.add(0., &thump(110., 0.03, 0.5), 1.);
    m.add(0.17, &clack(rng, 1300., 0.014, 1.), 1.);
    m.add(0.17, &thump(100., 0.035, 0.6), 1.);
    m.0
}

fn rocket_load(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.75);
    m.add(0., &rasp(rng, 0.3, 350., 600., 0.6), 1.);
    m.add(0.3, &thump(70., 0.06, 1.), 1.);
    m.add(0.3, &clack(rng, 700., 0.02, 0.7), 1.);
    m.add(0.5, &clack(rng, 1600., 0.01, 0.8), 1.);
    m.0
}

fn belt_load(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.9);
    m.add(0., &clack(rng, 700., 0.05, 1.), 1.);
    m.add(0., &thump(80., 0.05, 0.6), 1.);
    for k in 0..7 {
        let at = 0.22 + k as f32 * 0.045 + rng.range(0., 0.012);
        let f = rng.range(2200., 3400.);
        m.add(at, &clack(rng, f, 0.006, 0.5), 1.);
    }
    m.add(0.6, &clack(rng, 1000., 0.015, 1.), 1.);
    m.add(0.6, &thump(90., 0.04, 0.6), 1.);
    m.0
}

fn revolver_load(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.65);
    m.add(0., &clack(rng, 1500., 0.01, 0.9), 1.);
    let mut at = 0.12;
    for _ in 0..6 {
        m.add(at, &clack(rng, 2400., 0.004, 0.45), 1.);
        at += 0.035 + 0.008 * rng.f32();
    }
    m.add(0.5, &clack(rng, 1100., 0.012, 1.), 1.);
    m.add(0.5, &thump(110., 0.03, 0.5), 1.);
    m.0
}

fn break_action(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.85);
    m.add(0., &clack(rng, 900., 0.02, 1.), 1.);
    m.add(0., &thump(90., 0.04, 0.6), 1.);
    m.add(0.12, &clack(rng, 4200., 0.02, 0.35), 1.);
    m.add(0.38, &shell_insert(rng), 0.8);
    m.add(0.6, &clack(rng, 1000., 0.018, 1.), 1.);
    m.add(0.6, &thump(100., 0.04, 0.7), 1.);
    m.0
}

fn empty_click(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.12);
    m.add(0., &clack(rng, 2400., 0.005, 0.9), 1.);
    m.add(0.035, &clack(rng, 1500., 0.006, 0.3), 1.);
    m.0
}

fn weapon_draw(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.3);
    m.add(0., &whoosh(rng, 0.16, 700., 1500., 0.6, 1.0), 0.35);
    m.add(0.12, &clack(rng, 1500., 0.01, 0.8), 1.);
    m.add(0.12, &thump(100., 0.03, 0.4), 1.);
    m.0
}

fn ads(rng: &mut Rng, into: bool) -> Vec<f32> {
    let (a, b) = if into { (600., 1400.) } else { (1400., 600.) };
    let mut m = Mix::new(0.2);
    m.add(0., &whoosh(rng, 0.14, a, b, 0.6, 1.0), 0.4);
    m.add(if into { 0.12 } else { 0.0 }, &clack(rng, 2200., 0.005, 0.35), 1.);
    m.0
}

fn scope_zoom(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.16);
    for k in 0..3 {
        m.add(k as f32 * 0.045, &clack(rng, 3000. - k as f32 * 250., 0.004, 0.6), 1.);
    }
    m.0
}

// ------------------------------------------------------------------------------------------ movement

fn footstep(rng: &mut Rng, surf: Surface, p: f32) -> Vec<f32> {
    match surf {
        Surface::Metal => {
            let mut m = Mix::new(0.3);
            m.add(0., &burst(rng, 0.2, 500., 6000., 0.012), 0.5);
            m.add(
                0.,
                &modal(
                    0.3,
                    &[(620. * p, 0.6, 0.07), (1140. * p, 0.4, 0.05), (1830. * p, 0.25, 0.04), (2710. * p, 0.15, 0.03)],
                ),
                1.,
            );
            m.add(0., &thump(90. * p, 0.04, 0.4), 1.);
            m.0
        }
        Surface::Gravel => {
            let mut m = Mix::new(0.3);
            for _ in 0..12 {
                let t = rng.f32().powf(1.6) * 0.16;
                m.add(t, &burst(rng, 0.03, 1800., 6500., 0.006), rng.range(0.3, 1.));
            }
            m.add(0., &thump(70. * p, 0.04, 0.35), 1.);
            m.0
        }
        Surface::Grass | Surface::Dirt => {
            let mut m = Mix::new(0.25);
            m.add(0., &burst(rng, 0.2, 0., 900. * p, 0.05), 0.8);
            m.add(0., &thump(65. * p, 0.05, 0.6), 1.);
            for _ in 0..4 {
                m.add(rng.range(0.01, 0.12), &burst(rng, 0.03, 2000., 5000., 0.01), 0.12);
            }
            m.0
        }
        Surface::Wood => {
            let mut m = Mix::new(0.25);
            m.add(0., &modal(0.25, &[(190. * p, 0.7, 0.05), (340. * p, 0.5, 0.04), (560. * p, 0.3, 0.03)]), 1.);
            m.add(0., &burst(rng, 0.1, 600., 2500., 0.012), 0.5);
            m.add(0., &thump(110. * p, 0.03, 0.4), 1.);
            m.0
        }
        Surface::Water => water_splash(rng, p, 0.45),
        _ => {
            let mut m = Mix::new(0.22);
            m.add(0., &burst(rng, 0.12, 300. * p, 3500. * p, 0.018), 0.8);
            m.add(0., &thump(75. * p, 0.05, 0.75), 1.);
            m.0
        }
    }
}

fn water_splash(rng: &mut Rng, p: f32, dur: f32) -> Vec<f32> {
    let mut m = Mix::new(dur);
    let mut b = burst(rng, dur, 400., 4500. * p, 0.09);
    mul(&mut b, |t| 0.7 + 0.3 * (TAU * 23. * t).sin());
    m.add(0., &b, 0.8);
    for k in 0..3 {
        let f0 = rng.range(400., 700.) * p;
        m.add(0.03 + k as f32 * 0.06, &sweep(0.12, f0, f0 * 2.4, 0.04, 0.04, 0.), 0.25);
    }
    m.add(0., &thump(80., 0.05, 0.4), 1.);
    m.0
}

fn jump(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.3);
    m.add(0., &burst(rng, 0.2, 0., 1400., 0.05), 0.5);
    m.add(0., &thump(90., 0.04, 0.35), 1.);
    m.add(0.04, &burst(rng, 0.12, 1500., 4500., 0.03), 0.18);
    m.0
}

fn land(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.3);
    m.add(0., &thump(62., 0.07, 1.), 1.);
    m.add(0., &burst(rng, 0.2, 0., 1200., 0.04), 0.6);
    m.add(0.01, &clack(rng, 1800., 0.012, 0.3), 1.);
    m.0
}

// ------------------------------------------------------------------------------------------ impacts

fn impact_concrete(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.35);
    m.add(0., &burst(rng, 0.1, 1500., 8000., 0.012), 1.);
    m.add(0., &thump(120., 0.03, 0.35), 1.);
    for _ in 0..7 {
        m.add(rng.range(0.03, 0.25), &burst(rng, 0.02, 2500., 8000., 0.004), rng.range(0.1, 0.4));
    }
    m.0
}

fn impact_metal(rng: &mut Rng) -> Vec<f32> {
    let f = rng.range(1700., 2100.);
    let mut m = Mix::new(0.5);
    m.add(0., &modal(0.5, &[(f, 1., 0.13), (f * 1.52, 0.6, 0.09), (f * 2.23, 0.35, 0.06), (f * 3.1, 0.2, 0.04)]), 1.);
    m.add(0., &burst(rng, 0.05, 2000., 9000., 0.005), 0.7);
    m.0
}

fn impact_wood(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.3);
    m.add(0., &modal(0.3, &[(300., 0.8, 0.04), (700., 0.5, 0.025), (1250., 0.25, 0.02)]), 1.);
    m.add(0., &burst(rng, 0.08, 500., 3500., 0.01), 0.6);
    m.add(0., &thump(120., 0.03, 0.5), 1.);
    m.0
}

fn impact_glass(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.7);
    m.add(0., &burst(rng, 0.08, 3000., 10_000., 0.008), 1.);
    for k in 0..9 {
        let f = rng.range(3500., 8500.);
        m.add(
            0.01 + k as f32 * rng.range(0.02, 0.05),
            &modal(0.25, &[(f, 1., rng.range(0.03, 0.12)), (f * 1.6, 0.4, 0.04)]),
            rng.range(0.15, 0.5),
        );
    }
    m.0
}

fn impact_dirt(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.3);
    m.add(0., &burst(rng, 0.2, 0., 1300., 0.05), 0.9);
    m.add(0., &thump(80., 0.05, 0.7), 1.);
    for _ in 0..5 {
        m.add(rng.range(0.03, 0.2), &burst(rng, 0.03, 1500., 4500., 0.008), 0.2);
    }
    m.0
}

fn impact_flesh(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.25);
    m.add(0., &thump(75., 0.045, 1.), 1.);
    m.add(0., &burst(rng, 0.15, 0., 450., 0.03), 0.7);
    m.0
}

fn helmet_clank(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.45);
    m.add(0., &modal(0.45, &[(1100., 1., 0.09), (2400., 0.5, 0.06), (3900., 0.25, 0.04)]), 1.);
    m.add(0., &burst(rng, 0.03, 1500., 8000., 0.004), 0.7);
    m.add(0., &thump(130., 0.03, 0.5), 1.);
    m.0
}

fn hit_confirm(head: bool) -> Vec<f32> {
    if head {
        let mut x = modal(0.12, &[(2600., 1., 0.02), (3900., 0.5, 0.015)]);
        x.iter_mut().for_each(|v| *v *= 0.9);
        x
    } else {
        modal(0.08, &[(1800., 1., 0.012)])
    }
}

fn hurt_thud(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.35);
    m.add(0., &thump(100., 0.06, 1.), 1.);
    m.add(0., &burst(rng, 0.3, 0., 350., 0.08), 0.6);
    m.add(0.02, &burst(rng, 0.15, 800., 2500., 0.03), 0.2);
    m.0
}

fn body_fall(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.7);
    m.add(0., &burst(rng, 0.3, 0., 800., 0.06), 0.5);
    m.add(0.02, &thump(70., 0.09, 1.), 1.);
    for _ in 0..5 {
        let (at, f, g) = (rng.range(0.05, 0.4), rng.range(1800., 3600.), rng.range(0.1, 0.3));
        m.add(at, &clack(rng, f, 0.008, g), 1.);
    }
    m.add(0.22, &thump(60., 0.05, 0.5), 1.);
    m.0
}

// ------------------------------------------------------------------------------------------ grenades

fn pin_pull(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.3);
    m.add(0., &rasp(rng, 0.09, 1500., 2500., 0.4), 1.);
    m.add(0.1, &modal(0.2, &[(3100., 1., 0.05), (5000., 0.5, 0.03)]), 0.6);
    m.add(0.1, &clack(rng, 2400., 0.006, 0.6), 1.);
    m.0
}

fn spoon_ping(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.35);
    m.add(0., &clack(rng, 2000., 0.005, 0.7), 1.);
    m.add(0.01, &modal(0.3, &[(2400., 1., 0.09), (3700., 0.6, 0.06), (5500., 0.3, 0.04)]), 0.7);
    m.0
}

fn throw(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.4);
    m.add(0., &whoosh(rng, 0.35, 400., 1200., 0.7, 0.8), 0.8);
    m.add(0., &burst(rng, 0.1, 0., 600., 0.03), 0.3);
    m.0
}

fn grenade_bounce(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.35);
    m.add(0., &modal(0.3, &[(700., 0.8, 0.06), (1450., 0.5, 0.045), (2300., 0.3, 0.03)]), 1.);
    m.add(0., &thump(110., 0.04, 0.6), 1.);
    m.add(0., &burst(rng, 0.05, 1000., 6000., 0.008), 0.5);
    m.0
}

/// Explosion: sharp crack, deep falling boom, low rumble, then falling debris. `far` gives the distant
/// version: darker, no crack, longer rumble.
fn explosion(rng: &mut Rng, far: bool) -> Vec<f32> {
    let dur = if far { 2.4 } else { 2.3 };
    let mut m = Mix::new(dur);
    if !far {
        m.add(0., &burst(rng, 0.3, 200., 7000., 0.03), 0.8);
    }
    m.add(0., &sweep(1.6, 75., 24., 0.2, 0.5, 0.35), 1.);
    m.add(0., &burst(rng, 1.2, 0., 550., 0.35), 0.7);
    let mut rumble = lp(&noise(rng, n(2.2)), 130.);
    scale_to_peak(&mut rumble, 1.);
    mul(&mut rumble, |t| ad(t, 0.06, 0.9));
    m.add(0., &rumble, 0.9);
    for _ in 0..28 {
        let t = 0.12 + rng.f32().powf(1.4) * 1.7;
        let f = rng.range(800., 4500.);
        let g = 0.5 * (-t / 0.9).exp() * rng.range(0.4, 1.);
        m.add(t, &burst(rng, 0.05, f * 0.6, f * 1.5, 0.012), g);
    }
    let mut x = m.0;
    soft_clip(&mut x, 1.5);
    if far {
        x = lp(&x, 450.);
    }
    reverb(&x, if far { 1.6 } else { 1.0 }, 0.3)
}

fn flashbang(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.3);
    m.add(0., &burst(rng, 0.25, 700., 9500., 0.02), 1.);
    m.add(0., &sweep(0.3, 130., 60., 0.03, 0.06, 0.3), 0.8);
    m.add(0., &burst(rng, 0.1, 3000., 10_000., 0.006), 0.6);
    let mut x = m.0;
    soft_clip(&mut x, 2.0);
    reverb(&x, 0.7, 0.45)
}

/// The ringing in the player's ears: a thin tone that rises slowly while it fades, plus a faint second
/// tone, under a muffled low thump.
fn flash_ring(rng: &mut Rng) -> Vec<f32> {
    let dur = 4.2;
    let mut ph = [0f64; 2];
    let mut x = gen(dur, |t| {
        let f1 = 3300. + 1500. * (1. - (-t / 1.6).exp());
        let f2 = 5900. + 600. * (1. - (-t / 2.0).exp());
        ph[0] += std::f64::consts::TAU * f1 as f64 / SR as f64;
        ph[1] += std::f64::consts::TAU * f2 as f64 / SR as f64;
        let a = (t / 0.03).min(1.) * (-t / 1.3).exp();
        (ph[0].sin() as f32 * 0.7 + ph[1].sin() as f32 * 0.25) * a
    });
    let thud = {
        let mut m = Mix::new(dur);
        m.add(0., &sweep(0.5, 120., 45., 0.05, 0.12, 0.), 0.5);
        m.add(0., &burst(rng, 0.3, 0., 700., 0.08), 0.3);
        m.0
    };
    for (a, b) in x.iter_mut().zip(&thud) {
        *a += b;
    }
    x
}

fn smoke_pop(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.4);
    m.add(0., &thump(85., 0.05, 1.), 1.);
    m.add(0., &burst(rng, 0.25, 0., 1800., 0.05), 0.6);
    m.add(0.03, &whoosh(rng, 0.3, 1500., 3500., 0.6, 0.6), 0.3);
    m.0
}

fn smoke_hiss(rng: &mut Rng) -> Vec<f32> {
    let dur = 1.8;
    let mut x = bp(&noise(rng, n(dur)), 4200., 0.5);
    scale_to_peak(&mut x, 1.);
    let flutter = lp(&noise(rng, n(dur)), 9.);
    let fp = peak(&flutter).max(1e-6);
    for (i, v) in x.iter_mut().enumerate() {
        let t = i as f32 / SR;
        let env = (t / 0.08).min(1.) * if t > 1.25 { (-(t - 1.25) / 0.2).exp() } else { 1. };
        *v *= env * (0.75 + 0.25 * flutter[i] / fp);
    }
    x
}

fn incen_ignite(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(1.0);
    let mut w = whoosh(rng, 0.6, 300., 1800., 0.7, 0.45);
    mul(&mut w, |t| ad(t, 0.05, 0.3));
    m.add(0., &w, 0.9);
    m.add(0., &thump(75., 0.06, 0.8), 1.);
    m.add(0.1, &fire_crackle(rng, 0.9), 0.4);
    m.0
}

fn fire_crackle(rng: &mut Rng, dur: f32) -> Vec<f32> {
    let mut m = Mix::new(dur);
    let mut bed = lp(&noise(rng, n(dur)), 500.);
    scale_to_peak(&mut bed, 0.25);
    m.add(0., &bed, 1.);
    for _ in 0..(dur * 28.) as usize {
        let f = rng.range(1500., 5500.);
        let (at, tau, g) = (rng.range(0., dur - 0.03), rng.range(0.002, 0.006), rng.range(0.15, 1.));
        m.add(at, &burst(rng, 0.03, f * 0.7, f * 1.4, tau), g);
    }
    m.0
}

// ------------------------------------------------------------------------------------------ melee

fn knife_swing(rng: &mut Rng) -> Vec<f32> {
    whoosh(rng, 0.2, 1200., 3500., 1.2, 0.7)
}

fn heavy_swing(rng: &mut Rng) -> Vec<f32> {
    let mut w = whoosh(rng, 0.35, 400., 1100., 0.9, 0.7);
    let low = whoosh(rng, 0.35, 150., 350., 0.8, 0.7);
    for (a, b) in w.iter_mut().zip(&low) {
        *a += 0.5 * b;
    }
    w
}

fn knife_hit(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.25);
    m.add(0., &thump(110., 0.035, 0.9), 1.);
    m.add(0., &burst(rng, 0.1, 1200., 5000., 0.012), 0.6);
    m.add(0.0, &rasp(rng, 0.08, 3500., 2500., 0.3), 1.);
    m.0
}

fn axe_hit(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.5);
    m.add(0., &thump(70., 0.08, 1.), 1.);
    m.add(0., &modal(0.4, &[(220., 0.6, 0.07), (520., 0.35, 0.05), (900., 0.2, 0.03)]), 0.7);
    m.add(0., &burst(rng, 0.12, 400., 3500., 0.015), 0.7);
    m.add(0., &modal(0.3, &[(2100., 0.3, 0.05)]), 0.4);
    m.0
}

fn crowbar_hit(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.8);
    m.add(0., &modal(0.8, &[(450., 1., 0.15), (1100., 0.6, 0.12), (1900., 0.4, 0.08), (3100., 0.2, 0.05)]), 1.);
    m.add(0., &thump(90., 0.05, 0.7), 1.);
    m.add(0., &burst(rng, 0.04, 1500., 8000., 0.005), 0.6);
    m.0
}

fn machete_hit(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.35);
    m.add(0., &thump(95., 0.05, 0.9), 1.);
    m.add(0., &burst(rng, 0.15, 800., 4500., 0.02), 0.7);
    m.add(0., &modal(0.3, &[(1300., 0.4, 0.06), (2500., 0.2, 0.04)]), 0.5);
    m.0
}

// ------------------------------------------------------------------------------------------ pickups, UI, flow

fn pickup_weapon(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.4);
    m.add(0., &rasp(rng, 0.08, 500., 900., 0.4), 1.);
    m.add(0.07, &clack(rng, 1300., 0.012, 1.), 1.);
    m.add(0.07, &thump(100., 0.04, 0.5), 1.);
    m.add(0.2, &clack(rng, 2000., 0.008, 0.6), 1.);
    m.0
}

fn pickup_ammo(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.4);
    for k in 0..6 {
        let (at, f, g) = (k as f32 * 0.04 + rng.range(0., 0.015), rng.range(3000., 4800.), rng.range(0.4, 0.9));
        m.add(at, &clack(rng, f, 0.006, g), 1.);
    }
    m.add(0.25, &clack(rng, 1100., 0.012, 0.7), 1.);
    m.0
}

fn menu_hover(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.06);
    m.add(0., &burst(rng, 0.04, 2000., 5000., 0.008), 0.5);
    m.add(0., &modal(0.06, &[(1500., 0.5, 0.01)]), 1.);
    m.0
}

fn menu_click() -> Vec<f32> {
    modal(0.12, &[(520., 1., 0.028), (1180., 0.45, 0.02)])
}

fn menu_confirm() -> Vec<f32> {
    let mut m = Mix::new(0.3);
    m.add(0., &modal(0.3, &[(620., 1., 0.07), (1250., 0.4, 0.05), (1870., 0.15, 0.03)]), 1.);
    m.add(0., &modal(0.05, &[(2500., 0.3, 0.008)]), 1.);
    m.0
}

fn menu_back() -> Vec<f32> {
    modal(0.15, &[(380., 1., 0.05), (760., 0.4, 0.03)])
}

/// A short rewind-style sweep: band-passed noise whose centre rushes upward, with a faint tape-like tone.
fn killcam_whoosh(rng: &mut Rng) -> Vec<f32> {
    let dur = 0.75;
    let mut w = whoosh(rng, dur, 250., 4200., 1.4, 1.6);
    // The tone runs the opposite way (rising) so the ear reads "rewind".
    let mut ph = 0f64;
    let rising = gen(dur, |t| {
        let f = 180. + 1500. * (t / dur).powi(2);
        ph += std::f64::consts::TAU * f as f64 / SR as f64;
        ph.sin() as f32 * (PI * (t / dur)).sin().powf(2.)
    });
    for (i, a) in w.iter_mut().enumerate() {
        *a += 0.12 * rising[i];
    }
    let mut m = Mix::new(dur + 0.1);
    m.add(0., &w, 1.);
    m.add(dur - 0.03, &thump(70., 0.04, 0.5), 0.6);
    m.0
}

fn match_start(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(2.4);
    m.add(0., &modal(1.2, &[(140., 0.8, 0.3), (300., 0.5, 0.2), (620., 0.3, 0.12)]), 1.);
    m.add(0., &thump(55., 0.12, 1.), 1.);
    m.add(0., &burst(rng, 0.05, 800., 6000., 0.005), 0.6);
    m.add(0.6, &modal(0.8, &[(180., 0.6, 0.2), (390., 0.4, 0.15)]), 0.7);
    m.add(0.6, &thump(65., 0.08, 0.7), 1.);
    let mut hiss = bp(&noise(rng, n(1.2)), 4500., 0.6);
    scale_to_peak(&mut hiss, 1.);
    mul(&mut hiss, |t| ad(t, 0.05, 0.4));
    m.add(0.25, &hiss, 0.35);
    let mut ph = 0f64;
    let hum = gen(1.9, |t| {
        let f = 50. + 55. * (t / 1.9);
        ph += std::f64::consts::TAU * f as f64 / SR as f64;
        (ph.sin() as f32 + 0.5 * (2. * ph).sin() as f32) * (PI * (t / 1.9)).sin().powf(1.5)
    });
    m.add(0.3, &hum, 0.4);
    let x = m.0;
    reverb(&x, 0.9, 0.3)
}

fn match_end(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(3.2);
    m.add(0., &sweep(1.5, 60., 26., 0.4, 0.6, 0.3), 1.);
    m.add(0., &modal(1.5, &[(110., 0.6, 0.4), (250., 0.4, 0.3), (470., 0.25, 0.2)]), 0.8);
    m.add(0., &burst(rng, 0.1, 500., 5000., 0.01), 0.7);
    let mut sc = rasp(rng, 0.9, 900., 350., 0.4);
    mul(&mut sc, |t| ad(t, 0.1, 0.5));
    m.add(0.3, &sc, 1.);
    let mut ph = 0f64;
    let hum = gen(2.6, |t| {
        let f = 110. - 65. * (t / 2.6);
        ph += std::f64::consts::TAU * f as f64 / SR as f64;
        (ph.sin() as f32 + 0.4 * (2. * ph).sin() as f32) * (PI * (t / 2.6)).sin().powf(0.8)
    });
    m.add(0.2, &hum, 0.35);
    reverb(&m.0, 1.4, 0.4)
}

fn respawn(rng: &mut Rng) -> Vec<f32> {
    let mut m = Mix::new(0.9);
    m.add(0., &thump(70., 0.06, 0.6), 1.);
    m.add(0.02, &whoosh(rng, 0.7, 500., 3000., 0.8, 0.7), 0.6);
    m.add(0.15, &burst(rng, 0.3, 0., 900., 0.12), 0.25);
    m.0
}

// ------------------------------------------------------------------------------------------ ambience one-shots

fn amb_clang(rng: &mut Rng) -> Vec<f32> {
    let f = rng.range(280., 520.);
    let ratios = [1., 2.76, 5.4, 8.93, 13.3];
    let parts: Vec<(f32, f32, f32)> = ratios
        .iter()
        .enumerate()
        .map(|(i, r)| (f * r * rng.range(0.98, 1.02), 1. / (1. + i as f32 * 0.7), 0.4 / (1. + i as f32 * 0.8)))
        .collect();
    let mut x = modal(1.6, &parts);
    x = lp(&x, rng.range(1800., 3200.));
    reverb(&x, 1.3, 0.6)
}

fn amb_creak(rng: &mut Rng) -> Vec<f32> {
    let dur = rng.range(1.2, 1.6);
    let base = rng.range(170., 320.);
    let wob = rng.range(1.5, 3.5);
    let stick = rng.range(24., 40.);
    let mut ph = 0f64;
    let x = gen(dur, |t| {
        let f = base * (1. + 0.25 * (t / dur) + 0.05 * (TAU * wob * t).sin());
        ph += std::f64::consts::TAU * f as f64 / SR as f64;
        let saw = ((ph / std::f64::consts::TAU).fract() * 2. - 1.) as f32;
        let slip = 0.6 + 0.4 * (TAU * stick * t).sin().signum();
        saw * slip * (PI * t / dur).sin().powf(1.3)
    });
    let y = bp(&x, rng.range(700., 1300.), 1.0);
    reverb(&y, 0.7, 0.3)
}

fn amb_drip(rng: &mut Rng) -> Vec<f32> {
    let f = rng.range(800., 1300.);
    let mut m = Mix::new(0.4);
    m.add(0., &sweep(0.25, f, f * 2.0, 0.03, 0.04, 0.), 1.);
    m.add(0., &burst(rng, 0.02, 2000., 6000., 0.003), 0.2);
    reverb(&m.0, 1.2, 0.7)
}

fn amb_steam(rng: &mut Rng) -> Vec<f32> {
    let dur = rng.range(1.3, 1.9);
    let mut h = whoosh(rng, dur, 3500., 5000., 0.5, 0.45);
    let low = lp(&noise(rng, n(dur)), 300.);
    let lpk = peak(&low).max(1e-6);
    let hl = h.len() as f32;
    for (i, v) in h.iter_mut().enumerate() {
        *v += 0.25 * low[i] / lpk * (PI * i as f32 / hl).sin();
    }
    h
}

fn amb_bird(rng: &mut Rng, take: usize) -> Vec<f32> {
    let calls = 2 + take;
    let mut m = Mix::new(0.4 + calls as f32 * 0.22);
    let f0 = rng.range(2800., 3600.);
    for k in 0..calls {
        let f1 = f0 * rng.range(1.25, 1.6);
        let dir = if (k + take).is_multiple_of(2) { 1. } else { -1. };
        let (a, b) = if dir > 0. { (f0, f1) } else { (f1, f0) };
        let mut ph = 0f64;
        let c = gen(0.14, |t| {
            let f = a + (b - a) * (t / 0.14) + 90. * (TAU * 38. * t).sin();
            ph += std::f64::consts::TAU * f as f64 / SR as f64;
            (ph.sin() as f32 + 0.25 * (2. * ph).sin() as f32) * (PI * (t / 0.14)).sin().powf(1.2)
        });
        m.add(0.05 + k as f32 * 0.2, &c, 1.0 - 0.1 * k as f32);
    }
    let x = lp(&m.0, 6500.);
    reverb(&x, 0.8, 0.35)
}

fn amb_pigeon(rng: &mut Rng) -> Vec<f32> {
    let flaps = 7 + rng.below(3);
    let mut m = Mix::new(0.9);
    let mut t = 0.0;
    let mut gap = 0.1;
    for k in 0..flaps {
        let g = 1. - k as f32 / flaps as f32 * 0.6;
        let mut b = burst(rng, 0.08, 250., 1800., 0.02);
        b.iter_mut().for_each(|v| *v *= g);
        m.add(t, &b, 1.);
        t += gap;
        gap *= 0.92;
    }
    m.0
}

/// Synthesise the mono takes of a cue (one per pan variant or ambience take is made later).
fn cue_mono(sfx: Sfx) -> Vec<Vec<f32>> {
    use Sfx::*;
    let mut rng = Rng::new(SEED.wrapping_add((sfx as u64 + 1).wrapping_mul(0x9E37_79B9)));
    let r = &mut rng;
    let one = |x: Vec<f32>| vec![x];
    match sfx {
        ShotRpg => one(rocket_launch(r)),
        ShotThumper => one(thumper_shot(r)),
        s if (s as usize) <= ShotPk as usize => one(gun(r, &gun_params(s))),
        MagOutLight => one(mag_out(r, false)),
        MagInLight => one(mag_in(r, false)),
        MagOutHeavy => one(mag_out(r, true)),
        MagInHeavy => one(mag_in(r, true)),
        BoltPull => one(bolt_pull(r)),
        BoltCycle => one(bolt_cycle(r)),
        SlideRelease => one(slide_release(r)),
        ShellInsert => one(shell_insert(r)),
        PumpRack => one(pump_rack(r)),
        RocketLoad => one(rocket_load(r)),
        BeltLoad => one(belt_load(r)),
        RevolverLoad => one(revolver_load(r)),
        BreakAction => one(break_action(r)),
        EmptyClick => one(empty_click(r)),
        WeaponDraw => one(weapon_draw(r)),
        AdsIn => one(ads(r, true)),
        AdsOut => one(ads(r, false)),
        ScopeZoom => one(scope_zoom(r)),
        StepConcreteL => one(footstep(r, Surface::Concrete, 0.94)),
        StepConcreteR => one(footstep(r, Surface::Concrete, 1.05)),
        StepMetalL => one(footstep(r, Surface::Metal, 0.94)),
        StepMetalR => one(footstep(r, Surface::Metal, 1.05)),
        StepGravelL => one(footstep(r, Surface::Gravel, 0.94)),
        StepGravelR => one(footstep(r, Surface::Gravel, 1.05)),
        StepGrassL => one(footstep(r, Surface::Grass, 0.94)),
        StepGrassR => one(footstep(r, Surface::Grass, 1.05)),
        StepWoodL => one(footstep(r, Surface::Wood, 0.94)),
        StepWoodR => one(footstep(r, Surface::Wood, 1.05)),
        StepWaterL => one(footstep(r, Surface::Water, 0.94)),
        StepWaterR => one(footstep(r, Surface::Water, 1.06)),
        WaterSplash => one(water_splash(r, 0.85, 0.6)),
        Jump => one(jump(r)),
        Land => one(land(r)),
        ImpactConcrete => one(impact_concrete(r)),
        ImpactMetal => one(impact_metal(r)),
        ImpactWood => one(impact_wood(r)),
        ImpactGlass => one(impact_glass(r)),
        ImpactDirt => one(impact_dirt(r)),
        ImpactFlesh => one(impact_flesh(r)),
        HelmetClank => one(helmet_clank(r)),
        HitConfirm => one(hit_confirm(false)),
        HitConfirmHead => one(hit_confirm(true)),
        HurtThud => one(hurt_thud(r)),
        BodyFall => one(body_fall(r)),
        BulletWhiz => one(bullet_whiz(r)),
        PinPull => one(pin_pull(r)),
        SpoonPing => one(spoon_ping(r)),
        Throw => one(throw(r)),
        GrenadeBounce => one(grenade_bounce(r)),
        Explosion => one(explosion(r, false)),
        ExplosionDistant => one(explosion(r, true)),
        FlashBang => one(flashbang(r)),
        FlashRing => one(flash_ring(r)),
        SmokePop => one(smoke_pop(r)),
        SmokeHiss => one(smoke_hiss(r)),
        IncenIgnite => one(incen_ignite(r)),
        FireCrackle => one(fire_crackle(r, 1.2)),
        RocketFly => one(rocket_fly(r)),
        KnifeSwing => one(knife_swing(r)),
        HeavySwing => one(heavy_swing(r)),
        KnifeHit => one(knife_hit(r)),
        AxeHit => one(axe_hit(r)),
        CrowbarHit => one(crowbar_hit(r)),
        MacheteHit => one(machete_hit(r)),
        PickupWeapon => one(pickup_weapon(r)),
        PickupAmmo => one(pickup_ammo(r)),
        MenuHover => one(menu_hover(r)),
        MenuClick => one(menu_click()),
        MenuConfirm => one(menu_confirm()),
        MenuBack => one(menu_back()),
        KillcamWhoosh => one(killcam_whoosh(r)),
        MatchStart => one(match_start(r)),
        MatchEnd => one(match_end(r)),
        Respawn => one(respawn(r)),
        AmbClang => (0..AMB_TAKES).map(|_| amb_clang(r)).collect(),
        AmbCreak => (0..AMB_TAKES).map(|_| amb_creak(r)).collect(),
        AmbDrip => (0..AMB_TAKES).map(|_| amb_drip(r)).collect(),
        AmbSteam => (0..AMB_TAKES).map(|_| amb_steam(r)).collect(),
        AmbBird => (0..AMB_TAKES).map(|k| amb_bird(r, k)).collect(),
        AmbPigeon => (0..AMB_TAKES).map(|_| amb_pigeon(r)).collect(),
        _ => unreachable!("every cue is handled above"),
    }
}

/// Target peak and end fade (ms) of a cue. Loud cues reach 0.93, quiet interface sounds stay lower.
fn level(sfx: Sfx) -> (f32, f32) {
    use Sfx::*;
    match sfx {
        MenuHover => (0.3, 8.),
        MenuClick | MenuBack => (0.45, 12.),
        MenuConfirm => (0.5, 25.),
        HitConfirm | HitConfirmHead => (0.55, 15.),
        StepConcreteL | StepConcreteR | StepMetalL | StepMetalR | StepGravelL | StepGravelR | StepGrassL
        | StepGrassR | StepWoodL | StepWoodR => (0.6, 20.),
        StepWaterL | StepWaterR | WaterSplash => (0.65, 25.),
        Jump | Land | AdsIn | AdsOut | ScopeZoom | WeaponDraw | EmptyClick => (0.5, 15.),
        AmbClang | AmbCreak | AmbDrip | AmbSteam | AmbBird | AmbPigeon => (0.6, 60.),
        FlashRing => (0.7, 400.),
        s if (s as usize) <= ShotThumper as usize => (0.8 + 0.13 * (cap(s) / 0.9).min(1.), 40.),
        Explosion | ExplosionDistant | FlashBang | MatchEnd => (0.95, 120.),
        _ => (0.8, 25.),
    }
}

/// Longest useful length of a cue in seconds: reverb tails are cut here (keeps memory down).
fn cap(sfx: Sfx) -> f32 {
    use Sfx::*;
    match sfx {
        s if (s as usize) <= ShotPk as usize => gun_params(s).dur,
        ShotRpg => 2.2,
        ShotThumper => 0.6,
        Explosion | ExplosionDistant => 2.5,
        FlashBang => 0.6,
        SmokeHiss => 1.8,
        FireCrackle => 1.2,
        MatchStart => 2.6,
        MatchEnd => 3.4,
        AmbClang => 2.2,
        AmbCreak => 1.6,
        AmbDrip => 1.0,
        AmbBird => 1.4,
        _ => 10.,
    }
}

fn take_mono(sfx: Sfx) -> Vec<Vec<f32>> {
    let (target, fade) = level(sfx);
    let len = n(cap(sfx));
    cue_mono(sfx)
        .into_iter()
        .map(|mut t| {
            t.truncate(len);
            finish(t, target, fade)
        })
        .collect()
}

/// Render every WAV variant of one cue (mono clips, or stereo pan variants for directional cues).
pub fn render_cue(sfx: Sfx) -> Vec<Vec<u8>> {
    let takes = take_mono(sfx);
    if is_directional(sfx) {
        (0..PAN_STEPS).map(|v| pan_wav(&takes[0], variant_pan(v))).collect()
    } else {
        takes.iter().map(|t| wav_bytes(t, RATE)).collect()
    }
}

/// The finished mono signal of a cue's first take (what the centre variant contains), for measurements.
pub fn cue_samples(sfx: Sfx) -> Vec<f32> {
    take_mono(sfx).swap_remove(0)
}

// ------------------------------------------------------------------------------------------ stems

/// Samples in one stem.
pub const STEM_LEN: usize = (STEM_SECONDS as usize) * RATE as usize;
const XF: usize = RATE as usize; // one second of equal-power cross-fade at the wrap

/// Turn a stream of `STEM_LEN + XF` samples (produced by a process that is periodic in `STEM_SECONDS`)
/// into a seamless loop: the first second is cross-faded with the continuation past the end, so that the
/// last output sample is followed naturally by the first.
fn xfade_loop(ext: &[f32]) -> Vec<f32> {
    let mut out = ext[..STEM_LEN].to_vec();
    for i in 0..XF {
        let w = i as f32 / XF as f32 * FRAC_PI_2;
        out[i] = ext[i] * w.sin() + ext[STEM_LEN + i] * w.cos();
    }
    out
}

fn ext_noise(rng: &mut Rng) -> Vec<f32> {
    noise(rng, STEM_LEN + XF)
}

fn add_wrapped(dst: &mut [f32], at: f32, src: &[f32], g: f32) {
    let len = dst.len();
    let mut idx = n(at) % len;
    for v in src {
        dst[idx] += v * g;
        idx += 1;
        if idx == len {
            idx = 0;
        }
    }
}

/// Smooth periodic 0..1 gust curve (periods 16, 8 and 16/3 seconds, so it loops).
fn gust(t: f32) -> f32 {
    let s = |k: f32, ph: f32| 0.5 + 0.5 * (TAU * k * t / STEM_SECONDS + ph).sin();
    (0.6 * s(1., 0.7) + 0.25 * s(2., 2.1) + 0.15 * s(3., 4.0)).clamp(0., 1.)
}

/// Sum of sines at integer frequencies: exactly periodic over the stem. `parts` = (Hz, amplitude).
fn hum(parts: &[(f64, f32)], am_cycles: f64, am_depth: f32) -> Vec<f32> {
    (0..STEM_LEN)
        .map(|i| {
            let t = i as f64 / SR as f64;
            let mut s = 0f32;
            for &(f, a) in parts {
                s += a * (std::f64::consts::TAU * f * t).sin() as f32;
            }
            let am = 1. + am_depth * (std::f64::consts::TAU * am_cycles * t / STEM_SECONDS as f64).sin() as f32;
            s * am
        })
        .collect()
}

fn normalise_stem(x: &mut [f32], rms_target: f32, max_peak: f32) {
    let rms = (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt().max(1e-9);
    let mut g = rms_target / rms;
    let p = peak(x) * g;
    if p > max_peak {
        g *= max_peak / p;
    }
    x.iter_mut().for_each(|v| *v *= g);
}

fn stem_wind(rng: &mut Rng) -> Vec<f32> {
    let mut body = filt_var(&ext_noise(rng), Kind::Low, |t| 220. + 900. * gust(t), 0.8);
    scale_to_peak(&mut body, 1.);
    let mut rumble = lp(&ext_noise(rng), 90.);
    scale_to_peak(&mut rumble, 1.);
    let mut whistle = filt_var(&ext_noise(rng), Kind::Band, |t| 1300. + 600. * gust(t + 3.), 14.);
    scale_to_peak(&mut whistle, 1.);
    let mut hiss = bp(&ext_noise(rng), 3000., 0.4);
    scale_to_peak(&mut hiss, 1.);
    let mixed: Vec<f32> = (0..STEM_LEN + XF)
        .map(|i| {
            let t = i as f32 / SR;
            let g = gust(t);
            body[i] * (0.2 + 0.8 * g.powf(1.3))
                + rumble[i] * (0.25 + 0.4 * g)
                + whistle[i] * 0.22 * g.powi(3)
                + hiss[i] * 0.05 * g
        })
        .collect();
    let mut out = xfade_loop(&mixed);
    normalise_stem(&mut out, 0.12, 0.9);
    out
}

fn stem_machinery(rng: &mut Rng) -> Vec<f32> {
    let mut out = hum(&[(50., 0.5), (100., 0.3), (150., 0.12), (25., 0.25), (200., 0.04)], 4., 0.25);
    // Dull rumble and a slow steam-vent hiss that swells twice per loop.
    let mut rumble = lp(&ext_noise(rng), 120.);
    scale_to_peak(&mut rumble, 1.);
    let mut hiss = bp(&ext_noise(rng), 4200., 0.6);
    scale_to_peak(&mut hiss, 1.);
    let mixed: Vec<f32> = (0..STEM_LEN + XF)
        .map(|i| {
            let t = i as f32 / SR;
            let mut env = 0.;
            for c in [3.5f32, 11.0] {
                let d = ((t - c + 8.).rem_euclid(STEM_SECONDS)) - 8.;
                env += (-(d / 1.3).powi(2)).exp();
            }
            rumble[i] * 0.3 + hiss[i] * (0.02 + 0.16 * env)
        })
        .collect();
    for (o, m) in out.iter_mut().zip(xfade_loop(&mixed)) {
        *o += m;
    }
    // A pump beat each second and slow clanks at irregular times.
    let beat = sweep(0.4, 70., 42., 0.06, 0.12, 0.2);
    for k in 0..16 {
        add_wrapped(&mut out, k as f32, &beat, 0.45);
        if k % 2 == 1 {
            add_wrapped(&mut out, k as f32 + 0.24, &beat, 0.2);
        }
    }
    for k in 0..6 {
        let at = k as f32 * 2.6 + rng.range(0., 0.8);
        let f = rng.range(150., 260.);
        let c = modal(1.0, &[(f, 1., 0.25), (f * 2.76, 0.6, 0.15), (f * 5.4, 0.3, 0.08)]);
        let c = lp(&c, 1800.);
        add_wrapped(&mut out, at, &c, rng.range(0.25, 0.4));
    }
    normalise_stem(&mut out, 0.12, 0.9);
    out
}

fn stem_nature(rng: &mut Rng) -> Vec<f32> {
    // Leaves: high band-passed noise whose level follows a slow random envelope.
    let mut leaves = bp(&ext_noise(rng), 3200., 0.6);
    scale_to_peak(&mut leaves, 1.);
    let slow = lp(&ext_noise(rng), 6.);
    let sp = peak(&slow).max(1e-6);
    let mixed: Vec<f32> = (0..STEM_LEN + XF)
        .map(|i| {
            let t = i as f32 / SR;
            let e = (slow[i] / sp).abs();
            leaves[i] * (0.04 + 0.3 * e * e * (0.4 + 0.6 * gust(t)))
        })
        .collect();
    let mut out = xfade_loop(&mixed);
    // Crickets: bursts of 4 short pulses repeated, three insects at different pitches.
    for (f, period, g) in [(4300f32, 0.47f32, 0.06f32), (4650., 0.61, 0.045), (5100., 0.83, 0.035)] {
        let pulse = gen(0.016, |t| (TAU * f * t).sin() * (PI * t / 0.016).sin().powi(2));
        let mut t = rng.range(0., period);
        while t < STEM_SECONDS {
            for p in 0..4 {
                add_wrapped(&mut out, t + p as f32 * 0.032, &pulse, g);
            }
            t += period;
        }
    }
    // Far-off birds, a handful of short calls over the loop.
    for _ in 0..7 {
        let at = rng.range(0., STEM_SECONDS);
        let f0 = rng.range(2400., 4200.);
        let calls = 2 + rng.below(3);
        for k in 0..calls {
            let f1 = f0 * rng.range(1.2, 1.5);
            let up = rng.chance(0.5);
            let mut ph = 0f64;
            let c = gen(0.12, |t| {
                let k = t / 0.12;
                let f = if up { f0 + (f1 - f0) * k } else { f1 + (f0 - f1) * k };
                ph += std::f64::consts::TAU * f as f64 / SR as f64;
                ph.sin() as f32 * (PI * k).sin().powf(1.3)
            });
            let c = lp(&c, 5000.);
            add_wrapped(&mut out, at + k as f32 * 0.17, &c, 0.05);
        }
    }
    normalise_stem(&mut out, 0.1, 0.9);
    out
}

fn stem_interior(rng: &mut Rng) -> Vec<f32> {
    let mut out = hum(&[(40., 0.5), (60., 0.35), (80., 0.25), (120., 0.12), (180., 0.04)], 3., 0.15);
    let mut rumble = lp(&ext_noise(rng), 220.);
    scale_to_peak(&mut rumble, 1.);
    let mut air = bp(&ext_noise(rng), 650., 0.9);
    scale_to_peak(&mut air, 1.);
    let mixed: Vec<f32> = (0..STEM_LEN + XF)
        .map(|i| {
            let t = i as f32 / SR;
            rumble[i] * 0.3 + air[i] * 0.09 * (0.8 + 0.2 * (TAU * 2. * t / STEM_SECONDS).sin())
        })
        .collect();
    for (o, m) in out.iter_mut().zip(xfade_loop(&mixed)) {
        *o += m;
    }
    // Faint far-off clangs ringing in the big hall.
    for at in [2.0f32, 7.3, 12.1] {
        let f = rng.range(200., 380.);
        let c = modal(0.8, &[(f, 1., 0.3), (f * 2.76, 0.5, 0.2), (f * 5.4, 0.25, 0.1)]);
        let c = reverb(&lp(&c, 1500.), 2.6, 0.9);
        add_wrapped(&mut out, at, &c, 0.08);
    }
    normalise_stem(&mut out, 0.1, 0.9);
    out
}

fn render_stems() -> Vec<Vec<u8>> {
    let mut rng = Rng::new(SEED ^ 0x5713);
    let stems = [stem_wind(&mut rng), stem_machinery(&mut rng), stem_nature(&mut rng), stem_interior(&mut rng)];
    stems.iter().map(|s| wav_bytes(s, RATE)).collect()
}

/// The four stems as mono samples, for measurements.
pub fn stem_samples() -> Vec<Vec<f32>> {
    let mut rng = Rng::new(SEED ^ 0x5713);
    vec![stem_wind(&mut rng), stem_machinery(&mut rng), stem_nature(&mut rng), stem_interior(&mut rng)]
}

/// Synthesise everything. Takes a couple of seconds at most; run it on a worker thread.
pub fn render() -> Rendered {
    let all = Sfx::ALL;
    let threads = std::thread::available_parallelism().map_or(2, |p| p.get()).clamp(1, 8);
    let chunk = all.len().div_ceil(threads);
    let mut sfx: Vec<Vec<Vec<u8>>> = Vec::with_capacity(all.len());
    let mut stems = Vec::new();
    std::thread::scope(|s| {
        let handles: Vec<_> = all
            .chunks(chunk)
            .map(|part| s.spawn(move || part.iter().map(|&c| render_cue(c)).collect::<Vec<_>>()))
            .collect();
        let stem_handle = s.spawn(render_stems);
        for h in handles {
            sfx.extend(h.join().expect("audio worker panicked"));
        }
        stems = stem_handle.join().expect("stem worker panicked");
    });
    Rendered { sfx, stems }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    use vesper3d::viewer::devkit::synth::parse_wav;

    fn bank() -> &'static Rendered {
        static B: OnceLock<Rendered> = OnceLock::new();
        B.get_or_init(render)
    }

    fn samples(sfx: Sfx) -> &'static Vec<f32> {
        static S: OnceLock<Vec<Vec<f32>>> = OnceLock::new();
        &S.get_or_init(|| Sfx::ALL.iter().map(|&s| cue_samples(s)).collect())[sfx as usize]
    }

    fn st(sfx: Sfx) -> Stats {
        analyse(samples(sfx))
    }

    fn channels(wav: &[u8]) -> (u32, u16, Vec<f32>, Vec<f32>) {
        let (rate, ch, s) = parse_wav(wav).expect("valid wav");
        if ch == 1 {
            (rate, ch, s.clone(), s)
        } else {
            let l = s.iter().step_by(2).copied().collect();
            let r = s.iter().skip(1).step_by(2).copied().collect();
            (rate, ch, l, r)
        }
    }

    #[test]
    fn index_is_dense_and_ordered() {
        for (i, s) in Sfx::ALL.iter().enumerate() {
            assert_eq!(index(*s), i, "{}", s.name());
        }
        assert_eq!(bank().sfx.len(), Sfx::ALL.len());
        assert_eq!(PAN_STEPS % 2, 1);
        assert_eq!(pan_variant(-1.), 0);
        assert_eq!(pan_variant(0.), PAN_STEPS / 2);
        assert_eq!(pan_variant(1.), PAN_STEPS - 1);
        assert_eq!(pan_variant(f32::NAN), PAN_STEPS / 2);
    }

    #[test]
    fn roster_keys_have_shots() {
        for key in [
            "k9", "m45", "hc50", "rv357", "mp9", "ump", "pdw", "vkr", "k47", "m4c", "fm2", "bpa", "gl4", "dmr20",
            "svd", "scout", "awm", "m82", "pump12", "auto12", "sawn", "para", "pk", "rpg", "thumper",
        ] {
            assert!(shot_for_key(key).is_some(), "{key}");
            assert!(!reload_sequence(key).is_empty(), "{key} reload");
        }
        for key in ["frag", "flash", "smoke", "incen", "knife", "machete", "axe", "crowbar"] {
            assert!(shot_for_key(key).is_none());
        }
        for key in ["knife", "machete", "axe", "crowbar"] {
            assert!(melee_hit_for_key(key).is_some());
        }
        let mut seen = std::collections::HashSet::new();
        for s in Sfx::ALL.iter().take(25) {
            assert!(seen.insert(*s));
        }
    }

    #[test]
    fn every_cue_is_a_valid_wav_with_right_variants() {
        for (i, &sfx) in Sfx::ALL.iter().enumerate() {
            let v = &bank().sfx[i];
            assert_eq!(v.len(), variant_count(sfx), "{}", sfx.name());
            let want_ch = if is_directional(sfx) { 2 } else { 1 };
            let mut lens = Vec::new();
            for w in v {
                assert_eq!(&w[..4], b"RIFF");
                let (rate, ch, l, r) = channels(w);
                assert_eq!(rate, RATE);
                assert_eq!(ch, want_ch, "{}", sfx.name());
                assert!(!l.is_empty());
                assert_eq!(l.len(), r.len());
                let p = peak(&l).max(peak(&r));
                assert!(p <= 0.95 + 1e-3, "{} peak {p}", sfx.name());
                assert!(p > 0.05, "{} silent", sfx.name());
                // No click at either end (2 ms fade in, fade out).
                assert!(l[0].abs() < 0.02 && r[0].abs() < 0.02, "{} start", sfx.name());
                assert!(l[l.len() - 1].abs() < 0.02 && r[r.len() - 1].abs() < 0.02, "{} end", sfx.name());
                lens.push(l.len());
            }
            if is_directional(sfx) {
                // Pan variants differ only by the far-ear delay, 12 samples at most.
                let (lo, hi) = (lens.iter().min().unwrap(), lens.iter().max().unwrap());
                assert!(hi - lo <= 12, "{}", sfx.name());
            }
        }
    }

    #[test]
    fn peak_and_rms_bounds() {
        for &sfx in Sfx::ALL {
            let s = st(sfx);
            assert!(s.peak <= 0.95 + 1e-3 && s.peak >= 0.25, "{} peak {}", sfx.name(), s.peak);
            assert!(s.rms > 0.003, "{} rms {}", sfx.name(), s.rms);
            assert!(s.rms < 0.6, "{} rms {}", sfx.name(), s.rms);
        }
    }

    #[test]
    fn durations_are_short() {
        for &sfx in Sfx::ALL {
            let d = st(sfx).seconds;
            let long = matches!(
                sfx,
                Sfx::ShotScout
                    | Sfx::ShotAwm
                    | Sfx::ShotM82
                    | Sfx::Explosion
                    | Sfx::ExplosionDistant
                    | Sfx::FlashRing
                    | Sfx::MatchStart
                    | Sfx::MatchEnd
                    | Sfx::ShotRpg
            );
            let limit = if long { 4.5 } else { 2.3 };
            assert!(d > 0.04 && d <= limit, "{} {d}", sfx.name());
            let gunlike = (sfx as usize) <= Sfx::ShotThumper as usize;
            if !long
                && !gunlike
                && !matches!(
                    sfx,
                    Sfx::SmokeHiss
                        | Sfx::FireCrackle
                        | Sfx::IncenIgnite
                        | Sfx::AmbClang
                        | Sfx::AmbSteam
                        | Sfx::AmbCreak
                        | Sfx::AmbBird
                )
            {
                assert!(d <= 1.01, "{} {d}", sfx.name());
            }
        }
        for s in [Sfx::ShotK9, Sfx::ShotPdw, Sfx::ShotMp9, Sfx::StepConcreteL, Sfx::MenuClick] {
            assert!(st(s).seconds < 0.6, "{}", s.name());
        }
        assert!(st(Sfx::ShotM82).seconds > 1.5);
        assert!(st(Sfx::Explosion).seconds > 1.5);
    }

    #[test]
    fn big_weapons_are_darker_and_longer() {
        let c = |s| st(s).centroid;
        let d = |s| st(s).decay;
        assert!(c(Sfx::ShotM82) < c(Sfx::ShotPdw), "m82 {} pdw {}", c(Sfx::ShotM82), c(Sfx::ShotPdw));
        assert!(c(Sfx::ShotM82) < c(Sfx::ShotK9));
        assert!(c(Sfx::ShotAwm) < c(Sfx::ShotMp9));
        assert!(c(Sfx::ShotHc50) < c(Sfx::ShotK9));
        assert!(c(Sfx::ShotPk) < c(Sfx::ShotMp9));
        assert!(c(Sfx::ShotPdw) > c(Sfx::ShotUmp));
        assert!(c(Sfx::ShotSawn) < c(Sfx::ShotPdw));
        assert!(d(Sfx::ShotM82) > d(Sfx::ShotK9) * 2., "{} {}", d(Sfx::ShotM82), d(Sfx::ShotK9));
        assert!(d(Sfx::ShotScout) > d(Sfx::ShotM45));
        assert!(d(Sfx::ShotAwm) > d(Sfx::ShotM4c));
        assert!(d(Sfx::ShotGl4) > d(Sfx::ShotMp9));
        assert!(d(Sfx::ShotK47) > d(Sfx::ShotPdw));
        // Class means: sniper > rifle > SMG in decay, pistol/SMG brighter than snipers.
        let mean = |v: &[Sfx], f: &dyn Fn(Sfx) -> f32| v.iter().map(|&s| f(s)).sum::<f32>() / v.len() as f32;
        let snipers = [Sfx::ShotScout, Sfx::ShotAwm, Sfx::ShotM82];
        let rifles = [Sfx::ShotK47, Sfx::ShotM4c, Sfx::ShotFm2, Sfx::ShotBpa];
        let smgs = [Sfx::ShotMp9, Sfx::ShotUmp, Sfx::ShotPdw, Sfx::ShotVkr];
        let pistols = [Sfx::ShotK9, Sfx::ShotM45];
        assert!(mean(&snipers, &d) > mean(&rifles, &d));
        assert!(mean(&rifles, &d) > mean(&smgs, &d));
        assert!(mean(&rifles, &d) > mean(&pistols, &d));
        assert!(mean(&smgs, &c) > mean(&snipers, &c));
        assert!(mean(&pistols, &c) > mean(&snipers, &c));
    }

    #[test]
    fn every_weapon_sounds_different() {
        let shots: Vec<Sfx> = Sfx::ALL[..25].to_vec();
        for (i, &a) in shots.iter().enumerate() {
            for &b in &shots[i + 1..] {
                let (sa, sb) = (st(a), st(b));
                let dc = (sa.centroid / sb.centroid).ln().abs();
                let dd = (sa.decay / sb.decay).ln().abs();
                let dr = (sa.rms / sb.rms).ln().abs();
                assert!(
                    dc > 0.03 || dd > 0.06 || dr > 0.1,
                    "{} vs {}: centroid {:.0}/{:.0}, decay {:.2}/{:.2}",
                    a.name(),
                    b.name(),
                    sa.centroid,
                    sb.centroid,
                    sa.decay,
                    sb.decay
                );
                assert_ne!(samples(a), samples(b));
            }
        }
    }

    #[test]
    fn surfaces_have_their_character() {
        let c = |s| st(s).centroid;
        assert!(c(Sfx::ImpactFlesh) < c(Sfx::ImpactConcrete));
        assert!(c(Sfx::ImpactFlesh) < c(Sfx::ImpactMetal));
        assert!(c(Sfx::ImpactGlass) > c(Sfx::ImpactWood));
        assert!(c(Sfx::ImpactMetal) > c(Sfx::ImpactDirt));
        assert!(c(Sfx::StepMetalL) > c(Sfx::StepGrassL));
        assert!(c(Sfx::StepGravelL) > c(Sfx::StepGrassL));
        assert!(c(Sfx::StepWoodL) < c(Sfx::StepMetalL));
        assert!(st(Sfx::ImpactMetal).decay > st(Sfx::ImpactFlesh).decay);
        // Feet differ.
        assert_ne!(samples(Sfx::StepConcreteL), samples(Sfx::StepConcreteR));
        assert!(c(Sfx::Explosion) < c(Sfx::ShotK47), "{} {}", c(Sfx::Explosion), c(Sfx::ShotK47));
        assert!(c(Sfx::ExplosionDistant) < c(Sfx::Explosion));
        assert!(c(Sfx::FlashBang) > c(Sfx::Explosion));
        assert!(c(Sfx::FlashRing) > 3000., "{}", c(Sfx::FlashRing));
        assert!(st(Sfx::FlashRing).decay > 2.0);
        assert!(c(Sfx::HitConfirmHead) > c(Sfx::HitConfirm));
    }

    #[test]
    fn flash_ring_rises() {
        let x = samples(Sfx::FlashRing);
        let n1 = 2 * RATE as usize / 4; // 0.5 s windows
        let early = spectral_centroid(&x[n1 / 4..n1]);
        let late = spectral_centroid(&x[RATE as usize..RATE as usize + n1]);
        assert!(late > early + 200., "early {early} late {late}");
    }

    #[test]
    fn ui_is_quiet_and_soft() {
        for s in [Sfx::MenuHover, Sfx::MenuClick, Sfx::MenuConfirm, Sfx::MenuBack] {
            let x = st(s);
            assert!(x.peak <= 0.5 + 1e-3, "{}", s.name());
            assert!(x.seconds < 0.35);
        }
        assert!(st(Sfx::MenuHover).peak < st(Sfx::MenuConfirm).peak);
    }

    #[test]
    fn stems_loop_seamlessly() {
        let stems = &bank().stems;
        assert_eq!(stems.len(), STEM_COUNT);
        let decoded: Vec<Vec<f32>> = stems.iter().map(|w| channels(w).2).collect();
        for (i, s) in decoded.iter().enumerate() {
            assert_eq!(s.len(), STEM_LEN, "stem {i}");
            let p = peak(s);
            assert!(p <= 0.95 && p > 0.2, "stem {i} peak {p}");
            let rms = analyse(s).rms;
            assert!(rms > 0.03 && rms < 0.3, "stem {i} rms {rms}");
            // The step from the last sample to the first must look like any other step.
            let steps: Vec<f32> = s.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
            let mean_step = steps.iter().sum::<f32>() / steps.len() as f32;
            let max_step = steps.iter().fold(0f32, |m, v| m.max(*v));
            let seam = (s[0] - s[s.len() - 1]).abs();
            eprintln!("stem {i}: seam step {seam:.5}, mean step {mean_step:.5}, max step {max_step:.5}");
            assert!(seam <= max_step * 1.05 + 1e-4, "stem {i} seam {seam} max step {max_step}");
            assert!(seam < mean_step * 8. + 2e-3, "stem {i} seam {seam} mean step {mean_step}");
            // No fade at the ends: the loop's edges carry the same level as its middle.
            let edge = analyse(&s[..2000]).rms;
            assert!(edge > rms * 0.3, "stem {i} edge {edge} vs {rms}");
        }
        // Wind is low and dark, nature is bright, interior darkest.
        let c: Vec<f32> = decoded.iter().map(|s| spectral_centroid(&s[..65_536])).collect();
        assert!(c[STEM_NATURE] > c[STEM_WIND], "{c:?}");
        assert!(c[STEM_INTERIOR] < c[STEM_WIND], "{c:?}");
        assert!(c[STEM_MACHINERY] < c[STEM_NATURE], "{c:?}");
    }

    #[test]
    fn pan_variants_lean_the_right_way() {
        for &sfx in Sfx::ALL.iter().filter(|s| is_directional(**s)) {
            let v = &bank().sfx[sfx as usize];
            let energy = |w: &[u8]| {
                let (_, _, l, r) = channels(w);
                (l.iter().map(|x| x * x).sum::<f32>(), r.iter().map(|x| x * x).sum::<f32>())
            };
            let (l0, r0) = energy(&v[0]);
            assert!(l0 > 4. * r0, "{} hard left {l0} {r0}", sfx.name());
            let (l1, r1) = energy(&v[PAN_STEPS - 1]);
            assert!(r1 > 4. * l1, "{} hard right", sfx.name());
            let (lc, rc) = energy(&v[PAN_STEPS / 2]);
            assert!((lc / rc - 1.).abs() < 0.02, "{} centre {lc} {rc}", sfx.name());
            // Monotonic lean across the variants.
            let mut last = f32::MAX;
            for w in v {
                let (l, r) = energy(w);
                let ratio = l / (l + r);
                assert!(ratio <= last + 1e-4, "{}", sfx.name());
                last = ratio;
            }
        }
    }

    #[test]
    fn deterministic() {
        for s in [Sfx::ShotM82, Sfx::Explosion, Sfx::AmbBird, Sfx::StepGravelR, Sfx::MatchStart] {
            assert_eq!(render_cue(s), render_cue(s), "{}", s.name());
        }
        assert_eq!(stem_samples(), stem_samples());
    }

    #[test]
    fn ambience_takes_differ() {
        for s in [Sfx::AmbClang, Sfx::AmbCreak, Sfx::AmbDrip, Sfx::AmbSteam, Sfx::AmbBird, Sfx::AmbPigeon] {
            let v = &bank().sfx[s as usize];
            assert_eq!(v.len(), 3);
            assert_ne!(v[0], v[1], "{}", s.name());
            assert_ne!(v[1], v[2], "{}", s.name());
        }
    }

    #[test]
    fn memory_is_sane_and_render_is_quick() {
        let b = bank();
        let (mut bytes, mut decoded) = (0usize, 0usize);
        for w in b.sfx.iter().flatten().chain(b.stems.iter()) {
            bytes += w.len();
            let (_, _, l, _) = channels(w);
            // The engine keeps 44.1 kHz stereo f32 after decoding.
            decoded += l.len() * 2 * 2 * 4;
        }
        eprintln!(
            "audio bank: {:.1} MB of WAV, about {:.0} MB decoded by the engine",
            bytes as f64 / 1e6,
            decoded as f64 / 1e6
        );
        assert!(bytes < 60_000_000, "{bytes}");
        assert!(decoded < 180_000_000, "{decoded}");
        let t = std::time::Instant::now();
        let _ = render();
        let secs = t.elapsed().as_secs_f32();
        eprintln!("render() took {secs:.2} s");
        assert!(secs < 12., "{secs}");
    }
}
