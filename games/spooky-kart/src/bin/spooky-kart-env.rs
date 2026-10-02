//! Headless agent environment for Spooky Kart: roll out a policy, record and replay trajectories, validate
//! the env, measure its speed. No window, no sound, no network.
//!
//!   spooky-kart-env rollout  [--policy bot|random|idle|lookahead] [--seed N] [--difficulty easy|medium|hard]
//!                            [--agent-slot K[,K...]] [--frame-skip N] [--max-ticks N] [--out FILE.jsonl]
//!                            [--no-obs] [--hash-every K] [--bot-difficulty D]
//!   spooky-kart-env replay   FILE.jsonl
//!   spooky-kart-env validate [--seed N] [--difficulty D] [--agent-slot K[,K...]]
//!   spooky-kart-env bench    [--seed N] [--ticks N] [--difficulty D] [--agent-slot K]
//!   spooky-kart-env eval     [--policy P[,P...]] [--seeds A..B] [--difficulty D] [--agent-slot K[,K...]|all]
//!                            [--bot-difficulty D]
//!
//! `bot` is the game's own AI reading the whole sim (the privileged reference, at `--bot-difficulty`, default
//! the rivals' `--difficulty`); `lookahead` uses only the observation (and drives at Medium tuning). `rollout` prints one JSON summary line; `replay` exits non-zero on a divergence.
use spooky_kart::env::{validate, Env, EnvConfig};
use spooky_kart::policy::{self, Policy, Rollout};
use spooky_kart::recorder::{replay, Recorder, RecorderOptions, Trajectory};
use spooky_kart::{Difficulty, Sim};
use std::collections::HashMap;
use std::io::BufWriter;
use std::process::ExitCode;
use std::time::Instant;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

struct Flags(HashMap<String, String>);

impl Flags {
    fn parse(args: &[String]) -> Res<(Vec<String>, Flags)> {
        let (mut positional, mut map) = (Vec::new(), HashMap::new());
        let mut i = 0;
        while i < args.len() {
            if let Some(name) = args[i].strip_prefix("--") {
                if name == "no-obs" {
                    map.insert(name.to_string(), String::new());
                    i += 1;
                } else {
                    let value = args.get(i + 1).ok_or_else(|| format!("--{name} needs a value"))?;
                    map.insert(name.to_string(), value.clone());
                    i += 2;
                }
            } else {
                positional.push(args[i].clone());
                i += 1;
            }
        }
        Ok((positional, Flags(map)))
    }

    fn get<T: std::str::FromStr>(&self, name: &str, default: T) -> Res<T>
    where
        T::Err: std::fmt::Display,
    {
        match self.0.get(name) {
            Some(v) => v.parse().map_err(|e| format!("--{name} {v}: {e}").into()),
            None => Ok(default),
        }
    }

    fn slots(&self) -> Res<Vec<usize>> {
        match self.0.get("agent-slot") {
            Some(v) if v == "all" => Ok(vec![0]),
            Some(v) => {
                v.split(',').map(|s| s.trim().parse().map_err(|e| format!("--agent-slot {v}: {e}").into())).collect()
            }
            None => Ok(vec![0]),
        }
    }

    /// How the `bot` policy drives its kart: `--bot-difficulty`, else the rivals' difficulty.
    fn bot_level(&self, config: &EnvConfig) -> Res<Difficulty> {
        self.get("bot-difficulty", config.difficulty)
    }

    fn config(&self) -> Res<EnvConfig> {
        let defaults = EnvConfig::default();
        Ok(EnvConfig {
            seed: self.get("seed", defaults.seed)?,
            agent_slots: self.slots()?,
            difficulty: self.get("difficulty", Difficulty::default())?,
            max_ticks: self.get("max-ticks", defaults.max_ticks)?,
            frame_skip: self.get("frame-skip", defaults.frame_skip)?,
            ..defaults
        })
    }
}

fn summary(policy: &dyn Policy, env: &Env, r: &Rollout, wall: f64) -> serde_json::Value {
    serde_json::json!({
        "policy": policy.name(),
        "privileged": policy.privileged(),
        "seed": env.seed(),
        "difficulty": env.config().difficulty.name(),
        "agent_slots": env.agent_slots(),
        "frame_skip": env.config().frame_skip,
        "steps": r.steps,
        "ticks": r.ticks,
        "terminated": r.terminated,
        "truncated": r.truncated,
        "total_reward": r.total_reward,
        "place": r.places,
        "finish_seconds": r.finish_seconds,
        "state_hash": format!("{:#x}", r.state_hash),
        "wall_seconds": wall,
    })
}

fn rollout(flags: &Flags) -> Res<()> {
    let config = flags.config()?;
    let name = flags.get("policy", "bot".to_string())?;
    let mut policy = policy::by_name(&name, flags.bot_level(&config)?, config.seed)
        .ok_or("unknown policy (bot, random, idle, lookahead)")?;
    let mut env = Env::new(config)?;
    let options =
        RecorderOptions { hash_every: flags.get("hash-every", 1)?, record_obs: !flags.0.contains_key("no-obs") };
    let mut recorder = match flags.0.get("out") {
        Some(path) => Some(Recorder::new(BufWriter::new(std::fs::File::create(path)?), &env, options)?),
        None => None,
    };
    let started = Instant::now();
    let mut write_error = None;
    let mut last = None;
    let result = policy::rollout(&mut env, policy.as_mut(), |env, obs, actions, step| {
        if let Some(rec) = recorder.as_mut() {
            if let Err(e) = rec.record(env, obs, actions, step) {
                write_error.get_or_insert(e);
            }
        }
        if step.terminated || step.truncated {
            last = Some(step.clone());
        }
    })?;
    let wall = started.elapsed().as_secs_f64();
    if let Some(e) = write_error {
        return Err(e.into());
    }
    if let (Some(rec), Some(step)) = (recorder, last) {
        rec.finish(&env, &step)?;
    }
    println!("{}", summary(policy.as_ref(), &env, &result, wall));
    Ok(())
}

fn replay_file(positional: &[String]) -> Res<bool> {
    let path = positional.first().ok_or("usage: spooky-kart-env replay FILE.jsonl")?;
    let trajectory = Trajectory::read_file(path)?;
    let h = &trajectory.header;
    println!(
        "{}: {} v{}, seed {}, {:?} bots, agents {:?}, frame_skip {}, {} steps, hash every {} step(s), obs {}",
        path,
        h.game,
        h.version,
        h.seed,
        h.difficulty,
        h.agent_slots,
        h.frame_skip,
        trajectory.steps.len(),
        h.hash_every,
        if h.record_obs { "recorded" } else { "not recorded" }
    );
    let started = Instant::now();
    let r = replay(&trajectory);
    println!(
        "replayed {} steps in {:.2} s: {} state hashes, {} observations checked",
        r.steps_replayed,
        started.elapsed().as_secs_f64(),
        r.hashes_checked,
        r.obs_checked
    );
    match &r.first_divergence {
        Some(d) => println!("DIVERGED: {d}"),
        None => println!(
            "final state_hash {}, RaceReport {}",
            if r.final_hash_ok { "matches" } else { "MISSING/MISMATCH" },
            if r.report_ok { "matches" } else { "MISSING/MISMATCH" }
        ),
    }
    println!("{}", if r.ok() { "REPLAY OK" } else { "REPLAY FAILED" });
    Ok(r.ok())
}

/// Ticks per second of a policy through the env, over `ticks` ticks (resetting at the end of each race).
fn bench_policy(config: &EnvConfig, bot_level: Difficulty, name: &str, ticks: u64) -> Res<serde_json::Value> {
    let mut env = Env::new(config.clone())?;
    let mut policy = policy::by_name(name, bot_level, config.seed).ok_or("unknown policy")?;
    let mut obs = env.observe();
    let (mut done, mut races, mut seed) = (0u64, 0u64, config.seed);
    let started = Instant::now();
    while done < ticks {
        let before = env.ticks();
        let actions = policy.act(&env, &obs);
        let r = env.step(&actions)?;
        done += (env.ticks() - before) as u64;
        obs = r.obs;
        if r.terminated || r.truncated {
            races += 1;
            seed += 1;
            obs = env.reset(seed);
        }
    }
    let wall = started.elapsed().as_secs_f64();
    Ok(serde_json::json!({
        "what": format!("env + {name}"), "ticks": done, "races_finished": races, "wall_seconds": wall,
        "ticks_per_second": done as f64 / wall,
    }))
}

/// The same race with no env at all: every kart a bot, `Sim::step` straight (the cost of the game itself).
fn bench_sim(config: &EnvConfig, ticks: u64) -> serde_json::Value {
    let grid: Vec<_> = config.grid.iter().map(|c| (*c, spooky_kart::Driver::Bot)).collect();
    let idle = spooky_kart::Inputs::default();
    let (mut done, mut seed) = (0u64, config.seed);
    let mut sim = Sim::with_difficulty(seed, &grid, config.difficulty);
    let started = Instant::now();
    while done < ticks {
        sim.step(&idle);
        done += 1;
        if sim.is_over() {
            seed += 1;
            sim = Sim::with_difficulty(seed, &grid, config.difficulty);
        }
    }
    let wall = started.elapsed().as_secs_f64();
    serde_json::json!({ "what": "Sim::step only (8 bots, no env)", "ticks": done, "wall_seconds": wall, "ticks_per_second": done as f64 / wall })
}

fn bench(flags: &Flags) -> Res<()> {
    let config = flags.config()?;
    let ticks: u64 = flags.get("ticks", 60_000)?;
    let profile = if cfg!(debug_assertions) {
        "debug-assertions on (dev profile; the game crate is opt-level 2)"
    } else {
        "release"
    };
    println!("profile: {profile}");
    let sim = bench_sim(&config, ticks);
    println!("{sim}");
    let sim_rate = sim["ticks_per_second"].as_f64().unwrap_or(0.);
    for name in ["bot", "lookahead"] {
        let r = bench_policy(&config, flags.bot_level(&config)?, name, ticks)?;
        let tps = r["ticks_per_second"].as_f64().unwrap_or(0.);
        println!("{r}");
        println!(
            "  {name}: {:.0} ticks/s = {:.0}x real time; a ~6,000-tick race takes ~{:.0} ms ({:.1} races/s); Sim::step alone is {:.0} ticks/s",
            tps,
            tps / 60.,
            6000. / tps * 1000.,
            tps / 6000.,
            sim_rate
        );
    }
    Ok(())
}

fn eval(flags: &Flags) -> Res<()> {
    let config = flags.config()?;
    let policies = flags.get("policy", "bot,lookahead".to_string())?;
    let seeds = flags.get("seeds", "1..10".to_string())?;
    let (a, b) = seeds.split_once("..").ok_or("--seeds A..B")?;
    let (a, b): (u64, u64) = (a.parse()?, b.parse()?);
    // `--agent-slot all` evaluates each of the eight slots on its own (one agent a race), which averages
    // out that the grid's characters are not equally fast.
    let bot_level = flags.bot_level(&config)?;
    let slot_sets: Vec<Vec<usize>> = if flags.0.get("agent-slot").is_some_and(|v| v == "all") {
        (0..config.grid.len()).map(|s| vec![s]).collect()
    } else {
        vec![config.agent_slots.clone()]
    };
    for name in policies.split(',') {
        let (mut places, mut secs, mut unfinished) = (Vec::new(), Vec::new(), 0);
        for slots in &slot_sets {
            for seed in a..=b {
                let mut policy = policy::by_name(name, bot_level, seed).ok_or("unknown policy")?;
                let mut env = Env::new(EnvConfig { seed, agent_slots: slots.clone(), ..config.clone() })?;
                let r = policy::rollout(&mut env, policy.as_mut(), |_, _, _, _| {})?;
                for (p, s) in r.places.iter().zip(&r.finish_seconds) {
                    places.push(*p as f64);
                    match s {
                        Some(s) => secs.push(*s as f64),
                        None => unfinished += 1,
                    }
                }
            }
        }
        let mean = |v: &[f64]| if v.is_empty() { f64::NAN } else { v.iter().sum::<f64>() / v.len() as f64 };
        println!(
            "{}",
            serde_json::json!({
                "policy": name, "difficulty": config.difficulty.name(), "agent_slots": slot_sets,
                "seeds": format!("{a}..{b}"), "races": places.len(), "mean_place": mean(&places), "places": places,
                "mean_finish_seconds": mean(&secs), "did_not_finish": unfinished,
            })
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().cloned() else {
        eprintln!("usage: spooky-kart-env rollout|replay|validate|bench|eval ... (see the file header)");
        return ExitCode::from(2);
    };
    let run = || -> Res<bool> {
        let (positional, flags) = Flags::parse(&args[1..])?;
        match command.as_str() {
            "rollout" => rollout(&flags).map(|_| true),
            "replay" => replay_file(&positional),
            "validate" => {
                let report = validate(&flags.config()?);
                println!("{report}");
                Ok(report.passed())
            }
            "bench" => bench(&flags).map(|_| true),
            "eval" => eval(&flags).map(|_| true),
            other => Err(format!("unknown command '{other}'").into()),
        }
    };
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
