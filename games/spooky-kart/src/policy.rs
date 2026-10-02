//! Reference policies for the env, and the rollout loop that runs one.
//!
//! * [`ReferenceBot`]: the game's own AI, `bot::drive_as`, run through the env. It reads the whole `Sim`
//!   (it is **omniscient**: it sees every kart's hidden state), so it is the privileged reference to measure
//!   against, not an example of what an observation can do.
//! * [`Idle`] and [`Random`]: floor baselines.
//! * [`Lookahead`]: the obs-only baseline. It implements [`ObsPolicy`], whose `act` takes an [`Obs`] and
//!   nothing else, so it cannot reach the sim even by accident.
use crate::bot::{self, Difficulty};
use crate::env::{Env, EnvError, Obs, StepResult, LOOKAHEAD_M};
use crate::kart::KartInput;
use std::f32::consts::PI;

/// Something that picks one action per agent from the env.
pub trait Policy {
    fn name(&self) -> String;
    /// True when the policy reads state an observation does not carry (the omniscient reference).
    fn privileged(&self) -> bool {
        false
    }
    /// One action per agent slot, in `agent_slots` order. `env` is there for privileged policies only.
    fn act(&mut self, env: &Env, obs: &[Obs]) -> Vec<KartInput>;
}

/// A policy that sees one agent's observation and nothing else.
pub trait ObsPolicy {
    fn name(&self) -> String;
    fn act(&mut self, obs: &Obs) -> KartInput;
}

/// Adapts an [`ObsPolicy`] to every agent slot.
pub struct ObsOnly<P: ObsPolicy>(pub P);

impl<P: ObsPolicy> Policy for ObsOnly<P> {
    fn name(&self) -> String {
        self.0.name()
    }
    fn act(&mut self, _env: &Env, obs: &[Obs]) -> Vec<KartInput> {
        obs.iter().map(|o| self.0.act(o)).collect()
    }
}

/// The game's bot, run for the agent slots at `difficulty`. Privileged: it reads the whole sim.
pub struct ReferenceBot {
    pub difficulty: Difficulty,
}

impl Policy for ReferenceBot {
    fn name(&self) -> String {
        format!("bot-{}", self.difficulty.name().to_lowercase())
    }
    fn privileged(&self) -> bool {
        true
    }
    fn act(&mut self, env: &Env, _obs: &[Obs]) -> Vec<KartInput> {
        env.agent_slots().iter().map(|&slot| bot::drive_as(env.sim(), slot, self.difficulty)).collect()
    }
}

/// Does nothing: no gas, no steering.
pub struct Idle;

impl ObsPolicy for Idle {
    fn name(&self) -> String {
        "idle".into()
    }
    fn act(&mut self, _obs: &Obs) -> KartInput {
        KartInput::default()
    }
}

/// Uniform random steering and throttle, drift 20% of the time, perk 5%. Its own generator, seeded
/// explicitly, never the sim's.
pub struct Random {
    state: u64,
}

impl Random {
    pub fn new(seed: u64) -> Self {
        Self { state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1 }
    }
    fn next(&mut self) -> f32 {
        // xorshift64*: a uniform f32 in [0, 1).
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        let x = self.state.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (x >> 40) as f32 / (1u64 << 24) as f32
    }
}

impl ObsPolicy for Random {
    fn name(&self) -> String {
        "random".into()
    }
    fn act(&mut self, _obs: &Obs) -> KartInput {
        KartInput {
            throttle: self.next() * 2. - 1.,
            steer: self.next() * 2. - 1.,
            drift: self.next() < 0.2,
            perk: self.next() < 0.05,
        }
    }
}

/// The obs-only baseline: aim at the centreline a little ahead, slow for the bend ahead, never drift,
/// never use the perk. It is `bot::drive_as` re-derived from what a driver can see, minus drift and perks.
pub struct Lookahead {
    /// Metres aimed ahead: `look_base + speed * look_speed`.
    pub look_base: f32,
    pub look_speed: f32,
    pub bend_gain: f32,
    pub bend_cap: f32,
}

impl Default for Lookahead {
    fn default() -> Self {
        // The game's Medium tuning.
        Self { look_base: 10., look_speed: 0.65, bend_gain: 0.7, bend_cap: 0.40 }
    }
}

/// Linear interpolation of `values` (given at `LOOKAHEAD_M`) at distance `d`.
fn at_distance(values: impl Fn(usize) -> f32, d: f32) -> f32 {
    let last = LOOKAHEAD_M.len() - 1;
    if d <= LOOKAHEAD_M[0] {
        return values(0);
    }
    if d >= LOOKAHEAD_M[last] {
        return values(last);
    }
    let i = (0..last).find(|&i| d <= LOOKAHEAD_M[i + 1]).unwrap_or(last - 1);
    let t = (d - LOOKAHEAD_M[i]) / (LOOKAHEAD_M[i + 1] - LOOKAHEAD_M[i]);
    values(i) * (1. - t) + values(i + 1) * t
}

impl ObsPolicy for Lookahead {
    fn name(&self) -> String {
        "lookahead".into()
    }
    fn act(&mut self, obs: &Obs) -> KartInput {
        let own = &obs.own;
        let stats = own.character.stats();
        let look = self.look_base + own.speed * self.look_speed;
        let error = at_distance(|i| obs.lookahead[i].bearing, look).clamp(-PI, PI);
        let steer = (error * 2.2).clamp(-1., 1.);
        let bend = at_distance(|i| obs.lookahead[i].bend, look * 1.8).abs();
        let want = stats.top_speed * (1. - (bend * self.bend_gain).min(self.bend_cap)) * 0.995;
        let throttle = if own.speed < want {
            1.
        } else if own.speed > want * 1.25 {
            -0.5
        } else {
            0.
        };
        KartInput { throttle, steer, drift: false, perk: false }
    }
}

/// Build a policy by name: `bot` (privileged reference), `idle`, `random`, `lookahead` (obs-only).
pub fn by_name(name: &str, difficulty: Difficulty, seed: u64) -> Option<Box<dyn Policy>> {
    Some(match name {
        "bot" => Box::new(ReferenceBot { difficulty }),
        "idle" => Box::new(ObsOnly(Idle)),
        "random" => Box::new(ObsOnly(Random::new(seed))),
        "lookahead" => Box::new(ObsOnly(Lookahead::default())),
        _ => return None,
    })
}

/// How one episode went.
#[derive(Clone, Debug, PartialEq)]
pub struct Rollout {
    pub steps: u64,
    pub ticks: u32,
    pub terminated: bool,
    pub truncated: bool,
    /// Summed reward per agent.
    pub total_reward: Vec<f32>,
    /// Final race position per agent (1 = first); 0 if the episode was cut off before places were handed out.
    pub places: Vec<u32>,
    /// Seconds each agent took to finish, if it did.
    pub finish_seconds: Vec<Option<f32>>,
    pub state_hash: u64,
}

/// Run `policy` from the env's current (freshly reset) state until it is terminated or truncated.
/// `on_step(env, obs_before, actions, result)` sees every step (the recorder hangs here).
pub fn rollout(
    env: &mut Env,
    policy: &mut dyn Policy,
    mut on_step: impl FnMut(&Env, &[Obs], &[KartInput], &StepResult),
) -> Result<Rollout, EnvError> {
    let mut obs = env.observe();
    let mut total = vec![0.; env.agent_slots().len()];
    while !env.is_done() {
        let actions = policy.act(env, &obs);
        let result = env.step(&actions)?;
        for (t, r) in total.iter_mut().zip(&result.reward) {
            *t += r;
        }
        on_step(env, &obs, &actions, &result);
        obs = result.obs;
    }
    let report = env.report();
    let slots = env.agent_slots().to_vec();
    Ok(Rollout {
        steps: env.steps(),
        ticks: env.ticks(),
        terminated: env.sim().is_over(),
        truncated: !env.sim().is_over(),
        total_reward: total,
        places: slots.iter().map(|&s| report.racers[s].place).collect(),
        finish_seconds: slots.iter().map(|&s| report.racers[s].finish_seconds).collect(),
        state_hash: env.state_hash(),
    })
}
