//! `validate(config)`: the checks an env must pass before anything learns on it (the idea is PettingZoo's
//! `api_test`, adapted to this game). Each check names what it proves and reports a one-line detail.
use super::*;
use crate::policy::{self, ObsPolicy, Policy, Random};
use crate::sim::Hazard;
use vesper3d::math::V;

/// One check and how it went.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidationReport {
    pub checks: Vec<Check>,
}

impl ValidationReport {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }
}

impl std::fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for c in &self.checks {
            writeln!(f, "{} {:<34} {}", if c.ok { "ok  " } else { "FAIL" }, c.name, c.detail)?;
        }
        let bad = self.checks.iter().filter(|c| !c.ok).count();
        write!(f, "{} checks, {} failed", self.checks.len(), bad)
    }
}

type Outcome = Result<String, String>;

fn ensure(cond: bool, why: impl FnOnce() -> String) -> Result<(), String> {
    if cond {
        Ok(())
    } else {
        Err(why())
    }
}

/// Random actions for every agent, from the validation's own generator.
struct Noise(Random);

impl Noise {
    fn new(seed: u64) -> Self {
        Noise(Random::new(seed))
    }
    fn actions(&mut self, agents: usize, obs: &[Obs]) -> Vec<KartInput> {
        (0..agents).map(|i| self.0.act(&obs[i])).collect()
    }
}

fn vecs(obs: &[Obs]) -> Vec<Vec<f32>> {
    obs.iter().map(Obs::to_vec).collect()
}

fn in_bounds(v: &[f32]) -> Result<(), String> {
    ensure(v.len() == OBS_LEN, || format!("observation has {} floats, the layout says {OBS_LEN}", v.len()))?;
    let names = Obs::names();
    for (i, (x, (lo, hi))) in v.iter().zip(Obs::bounds()).enumerate() {
        ensure(x.is_finite(), || format!("'{}' is not finite ({x})", names[i]))?;
        ensure((lo..=hi).contains(x), || format!("'{}' = {x} is outside its bounds {lo}..{hi}", names[i]))?;
    }
    Ok(())
}

fn fresh(config: &EnvConfig) -> Result<Env, String> {
    Env::new(config.clone()).map_err(|e| e.to_string())
}

/// Run the checks on `config` (its grid, agent slots, difficulty and seed; frame skip and `max_ticks` are
/// varied by the checks that need to). Takes a few seconds in a dev build: it runs several full races.
pub fn validate(config: &EnvConfig) -> ValidationReport {
    let mut checks = Vec::new();
    let mut run = |name: &'static str, f: &dyn Fn() -> Outcome| {
        let (ok, detail) = match f() {
            Ok(d) => (true, d),
            Err(d) => (false, d),
        };
        checks.push(Check { name, ok, detail });
    };
    let agents = config.agent_slots.len();
    let seed = config.seed;

    run("config_and_layouts", &|| {
        config.check().map_err(|e| e.to_string())?;
        let env = fresh(config)?;
        let v = env.observe()[0].to_vec();
        ensure(OBS_LAYOUT.iter().map(|l| l.1).sum::<usize>() == OBS_LEN, || {
            "obs layout sizes do not sum to LEN".into()
        })?;
        ensure(Obs::names().len() == OBS_LEN && Obs::bounds().len() == OBS_LEN, || {
            "names or bounds have the wrong length".into()
        })?;
        ensure(Action::LAYOUT.iter().map(|l| l.1).sum::<usize>() == Action::LEN, || {
            "action layout sizes do not sum to LEN".into()
        })?;
        in_bounds(&v)?;
        Ok(format!("{OBS_LEN} obs floats per agent, {} action floats", Action::LEN))
    });

    run("reset_reproducible", &|| {
        let mut env = fresh(config)?;
        let first = vecs(&env.observe());
        let initial = env.state_hash();
        let play = |env: &mut Env| -> Result<u64, String> {
            let mut noise = Noise::new(7);
            let mut obs = env.observe();
            for _ in 0..600 {
                let a = noise.actions(agents, &obs);
                let r = env.step(&a).map_err(|e| e.to_string())?;
                obs = r.obs;
                if r.terminated || r.truncated {
                    break;
                }
            }
            Ok(env.state_hash())
        };
        let h1 = play(&mut env)?;
        let again = vecs(&env.reset(seed));
        ensure(again == first, || "reset(seed) gave a different first observation".into())?;
        ensure(env.state_hash() == initial, || "reset(seed) gave a different state hash".into())?;
        let h2 = play(&mut env)?;
        ensure(h1 == h2, || format!("the same actions after reset gave {h1:#x} then {h2:#x}"))?;
        env.reset(seed + 1);
        ensure(env.state_hash() != initial, || "a different seed gave the same race".into())?;
        Ok(format!("reset({seed}) twice: same observation, same hash {initial:#x}, same 600 steps ({h1:#x})"))
    });

    run("deterministic", &|| {
        let (mut a, mut b) = (fresh(config)?, fresh(config)?);
        let (mut na, mut nb) = (Noise::new(11), Noise::new(11));
        let (mut oa, mut ob) = (a.observe(), b.observe());
        let mut steps = 0;
        for i in 0..1500 {
            let (aa, ab) = (na.actions(agents, &oa), nb.actions(agents, &ob));
            let (ra, rb) = (a.step(&aa).map_err(|e| e.to_string())?, b.step(&ab).map_err(|e| e.to_string())?);
            ensure(vecs(&ra.obs) == vecs(&rb.obs), || format!("observations differ at step {i}"))?;
            ensure(ra.reward == rb.reward && ra.events == rb.events, || {
                format!("rewards or events differ at step {i}")
            })?;
            ensure(a.state_hash() == b.state_hash(), || format!("state hashes differ at step {i}"))?;
            steps += 1;
            if ra.terminated || ra.truncated {
                break;
            }
            (oa, ob) = (ra.obs, rb.obs);
        }
        Ok(format!("two envs, same seed and actions: identical obs, reward, events and hash for {steps} steps"))
    });

    run("observations_finite_and_bounded", &|| {
        let mut checked = 0;
        let mut reference = policy::ReferenceBot { difficulty: config.difficulty };
        let mut noise = Noise::new(3);
        for episode in 0..2 {
            let mut env = fresh(config)?;
            let mut obs = env.observe();
            while !env.is_done() {
                for o in &obs {
                    in_bounds(&o.to_vec())?;
                    checked += 1;
                }
                let a = if episode == 0 { reference.act(&env, &obs) } else { noise.actions(agents, &obs) };
                obs = env.step(&a).map_err(|e| e.to_string())?.obs;
                if episode == 1 && env.ticks() >= 3000 {
                    break;
                }
            }
        }
        Ok(format!("{checked} observations (a full reference-bot race and 3000 random-action ticks): right length, finite, in bounds"))
    });

    run("actions_clamped_nan_refused", &|| {
        let extreme = vec![KartInput { throttle: 9., steer: -7., drift: true, perk: false }; agents];
        let tame = vec![KartInput { throttle: 1., steer: -1., drift: true, perk: false }; agents];
        let (mut a, mut b) = (fresh(config)?, fresh(config)?);
        for _ in 0..180 {
            a.step(&extreme).map_err(|e| e.to_string())?;
            b.step(&tame).map_err(|e| e.to_string())?;
        }
        ensure(a.state_hash() == b.state_hash(), || "out-of-range actions were not clamped to the same race".into())?;
        let before = a.state_hash();
        let mut nan = tame.clone();
        nan[0].steer = f32::NAN;
        ensure(matches!(a.step(&nan), Err(EnvError::NonFinite { .. })), || "a NaN steer was accepted".into())?;
        nan[0].steer = 0.;
        nan[0].throttle = f32::INFINITY;
        ensure(matches!(a.step(&nan), Err(EnvError::NonFinite { .. })), || "an infinite throttle was accepted".into())?;
        ensure(matches!(a.step(&[]), Err(EnvError::ActionCount { .. })), || {
            "the wrong number of actions was accepted".into()
        })?;
        ensure(a.state_hash() == before && a.steps() == 180, || "a refused step changed the race".into())?;
        ensure(Action::from_vec(&[f32::NAN, 0., 0., 0.]).is_err(), || "Action::from_vec accepted NaN".into())?;
        Ok("throttle 9 / steer -7 plays exactly as 1 / -1; NaN, infinity and a wrong count are refused and change nothing".into())
    });

    run("frame_skip_is_repeated_ticks", &|| {
        let one = EnvConfig { frame_skip: 1, ..config.clone() };
        let four = EnvConfig { frame_skip: 4, ..config.clone() };
        let (mut a, mut b) = (fresh(&one)?, fresh(&four)?);
        let mut noise = Noise::new(5);
        let mut obs = a.observe();
        let (mut sum_a, mut sum_b) = (vec![0.; agents], vec![0.; agents]);
        for i in 0..200 {
            let mut act = noise.actions(agents, &obs);
            for x in &mut act {
                x.perk = false;
            }
            for _ in 0..4 {
                let r = a.step(&act).map_err(|e| e.to_string())?;
                for (s, r) in sum_a.iter_mut().zip(&r.reward) {
                    *s += r;
                }
                obs = r.obs;
            }
            let r = b.step(&act).map_err(|e| e.to_string())?;
            for (s, r) in sum_b.iter_mut().zip(&r.reward) {
                *s += r;
            }
            ensure(a.state_hash() == b.state_hash(), || {
                format!("frame_skip 4 differs from 4 single steps at step {i}")
            })?;
        }
        ensure(sum_a.iter().zip(&sum_b).all(|(x, y)| (x - y).abs() < 1e-3), || {
            format!("rewards differ: {sum_a:?} vs {sum_b:?}")
        })?;
        Ok("frame_skip 4 = four single steps with the same action: same hash every 4 ticks, same summed reward".into())
    });

    run("truncation", &|| {
        let cut = EnvConfig { max_ticks: 50, frame_skip: 1, ..config.clone() };
        let mut env = fresh(&cut)?;
        let idle = vec![KartInput::default(); agents];
        for i in 0..50 {
            let r = env.step(&idle).map_err(|e| e.to_string())?;
            ensure(r.truncated == (i == 49) && !r.terminated, || format!("truncated={} at step {i}", r.truncated))?;
        }
        ensure(matches!(env.step(&idle), Err(EnvError::EpisodeOver)), || {
            "stepping after truncation was allowed".into()
        })?;
        let skipping = EnvConfig { max_ticks: 50, frame_skip: 8, ..config.clone() };
        let mut env = fresh(&skipping)?;
        let mut steps = 0;
        while !env.is_done() {
            env.step(&idle).map_err(|e| e.to_string())?;
            steps += 1;
        }
        ensure(env.ticks() == 50 && steps == 7, || {
            format!("frame_skip 8, max_ticks 50 ran {} ticks in {steps} steps", env.ticks())
        })?;
        env.reset(seed);
        ensure(!env.is_done() && env.ticks() == 0, || "reset did not clear the truncation".into())?;
        Ok("truncated exactly at max_ticks (also mid frame-skip), not terminated, steps refused until reset".into())
    });

    run("terminated_at_race_end", &|| {
        let mut env = fresh(config)?;
        let mut reference = policy::ReferenceBot { difficulty: config.difficulty };
        let mut obs = env.observe();
        let mut total = vec![0.; agents];
        let mut last_events = Vec::new();
        while !env.is_done() {
            let a = reference.act(&env, &obs);
            let r = env.step(&a).map_err(|e| e.to_string())?;
            ensure(r.terminated == env.sim().is_over(), || "terminated disagrees with the sim".into())?;
            for (t, x) in total.iter_mut().zip(&r.reward) {
                *t += x;
            }
            obs = r.obs;
            last_events = r.events;
        }
        ensure(env.sim().is_over(), || "the episode ended before the race did".into())?;
        ensure(last_events.contains(&Event::RaceOver), || "the last step had no RaceOver event".into())?;
        ensure(matches!(env.step(&vec![KartInput::default(); agents]), Err(EnvError::EpisodeOver)), || {
            "stepping after the end was allowed".into()
        })?;
        let info = env.info();
        ensure(info.finished.iter().all(|f| *f), || format!("the reference bot did not finish: {:?}", info.finished))?;
        ensure(total.iter().all(|t| t.is_finite() && *t > 0.), || format!("total reward {total:?}"))?;
        let mut idle_env = fresh(&EnvConfig { max_ticks: 20_000, ..config.clone() })?;
        let idle = vec![KartInput::default(); agents];
        while !idle_env.is_done() {
            idle_env.step(&idle).map_err(|e| e.to_string())?;
        }
        ensure(idle_env.sim().is_over(), || "an idle agent's race did not end by the game's own rule".into())?;
        Ok(format!(
            "reference bot: terminated at tick {} (places {:?}, reward {:.1}); an idle agent's race also ends on its own at tick {}",
            env.ticks(),
            info.ranks,
            total[0],
            idle_env.ticks()
        ))
    });

    run("observation_ignores_hidden_state", &|| {
        let slot = config.agent_slots[0];
        let mut env = fresh(config)?;
        // A few seconds in, with a bone and a bandage on the road near the agent.
        let mut reference = policy::ReferenceBot { difficulty: config.difficulty };
        for _ in 0..120 {
            let obs = env.observe();
            let a = reference.act(&env, &obs);
            env.step(&a).map_err(|e| e.to_string())?;
        }
        let at = env.sim().karts[slot].pos;
        let owner = (slot + 1) % env.sim().karts.len();
        for (dx, kind) in [(6., HazardKind::Bone), (-9., HazardKind::Bandage)] {
            env.sim_mut().hazards.push(Hazard { pos: at + V(dx, 0., 4.), radius: 1.6, ttl: 400, owner, kind });
        }
        // Only the first agent's view is compared: another agent's own kart is observable to that agent.
        let before = vecs(&env.observe()).swap_remove(0);
        let seen = env.observe();
        let others = seen[0].others.iter().flatten().count();
        let hazards = seen[0].hazards.iter().flatten().count();
        ensure(others > 0 && hazards >= 2, || format!("the scenario shows {others} karts and {hazards} hazards"))?;
        // Change everything about the others and the hazards that a driver cannot see.
        let sim = env.sim_mut();
        for (j, k) in sim.karts.iter_mut().enumerate() {
            if j == slot {
                k.skill = 0.01;
                k.stats.perk_uses += 9;
                k.stats.top_speed += 5.;
                k.stats.wall_hits += 3;
                continue;
            }
            k.cooldown = 777;
            k.drift_charge = 1.9;
            k.drift_dir = -1.;
            k.boost_power = 0.9;
            k.slow_factor = 0.3;
            k.phase_ticks = 90;
            k.skill = 0.01;
            k.stats.perk_uses += 5;
            k.stats.collisions += 5;
            k.stats.wall_hits += 5;
            k.stats.distance += 999.;
            k.stats.laps.push(1);
        }
        for h in &mut sim.hazards {
            h.owner = slot;
            h.ttl += 1000;
        }
        let after = vecs(&env.observe()).swap_remove(0);
        ensure(before == after, || "the observation moved when only hidden state changed".into())?;
        // The positive control: moving an other kart must change it.
        let near = seen[0].others.iter().flatten().next().map(|o| o.distance).unwrap_or(0.);
        let target = (0..env.sim().karts.len())
            .find(|&j| j != slot && (env.sim().karts[j].pos - at).length() == near)
            .unwrap_or(owner);
        env.sim_mut().karts[target].pos = env.sim().karts[target].pos + V(0., 0., 2.5);
        let moved = vecs(&env.observe()).swap_remove(0);
        ensure(moved != after, || {
            "moving a visible kart did not change the observation (the check cannot see anything)".into()
        })?;
        Ok("other karts' perk timers, drift charge, boost, skill and counters, and hazard owners and lifetimes can change without moving the observation; moving a visible kart does".into())
    });

    ValidationReport { checks }
}
