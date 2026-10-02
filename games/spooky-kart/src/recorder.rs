//! Trajectory recording and replay, as JSON lines.
//!
//! A trajectory file is:
//!
//! 1. one `{"type":"header", ...}` line: game, version, content fingerprint, the seed and the whole env
//!    config (grid, agent slots, difficulty, frame skip, max ticks, reward weights), the observation and
//!    action layouts, the state hash right after `reset`, how often step hashes are written, and the
//!    determinism promise;
//! 2. one `{"type":"step", ...}` line per `Env::step`: `t` (the sim tick the observation was taken at),
//!    `obs` (the observation each agent acted on, omitted when recording without observations), `action`
//!    (the clamped action per agent, the exact numbers the env was given), `reward`, `events` and, every
//!    `hash_every` steps and on the last, `hash` (the sim `state_hash` after the step);
//! 3. one `{"type":"footer", ...}` line: steps, how it ended, the final `state_hash`, total reward per agent
//!    and the full `RaceReport`.
//!
//! [`replay`] rebuilds the env from the header, feeds the recorded actions and checks every hash it finds,
//! every recorded observation and reward, and the footer, reporting the first divergence.
use crate::bot::Difficulty;
use crate::character::Character;
use crate::env::{Action, Env, EnvConfig, EnvError, Event, Obs, StepResult, OBS_LAYOUT};
use crate::kart::KartInput;
use crate::sim::RaceReport;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

/// What every trajectory says about determinism.
pub const DETERMINISM: &str = "per machine; same binary and platform reproduce exactly";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub game: String,
    /// The `spooky-kart` crate version that wrote the file.
    pub version: String,
    pub content_fingerprint: u32,
    pub seed: u64,
    pub difficulty: Difficulty,
    pub grid: Vec<Character>,
    pub agent_slots: Vec<usize>,
    pub frame_skip: u32,
    pub max_ticks: u32,
    pub skip_countdown: bool,
    /// The whole config, so a replay rebuilds the same env (reward weights included).
    pub config: EnvConfig,
    pub obs_layout: Vec<(String, usize)>,
    pub action_layout: Vec<(String, usize)>,
    /// Steps between `hash` fields (1 = every step).
    pub hash_every: u32,
    /// Whether step lines carry `obs`.
    pub record_obs: bool,
    /// The state hash right after `reset(seed)`.
    pub initial_hash: u64,
    pub determinism: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepRecord {
    /// Sim tick when the observation was taken (before the action).
    pub t: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obs: Option<Vec<Vec<f32>>>,
    pub action: Vec<Vec<f32>>,
    pub reward: Vec<f32>,
    pub events: Vec<Event>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Footer {
    pub steps: u64,
    pub terminated: bool,
    pub truncated: bool,
    pub state_hash: u64,
    pub total_reward: Vec<f32>,
    pub report: RaceReport,
}

/// One line of a trajectory file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Line {
    Header(Header),
    Step(StepRecord),
    Footer(Footer),
}

/// What to write.
#[derive(Clone, Copy, Debug)]
pub struct RecorderOptions {
    /// Write a state hash every this many steps (and always on the last). 1 = every step.
    pub hash_every: u32,
    /// Write the observations (about 90 floats per agent per step). Replay does not need them.
    pub record_obs: bool,
}

impl Default for RecorderOptions {
    fn default() -> Self {
        Self { hash_every: 1, record_obs: true }
    }
}

pub fn header_for(env: &Env, options: RecorderOptions) -> Header {
    let c = env.config();
    Header {
        game: "spooky-kart".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        content_fingerprint: crate::content_fingerprint(),
        seed: env.seed(),
        difficulty: c.difficulty,
        grid: c.grid.clone(),
        agent_slots: c.agent_slots.clone(),
        frame_skip: c.frame_skip,
        max_ticks: c.max_ticks,
        skip_countdown: c.skip_countdown,
        config: EnvConfig { seed: env.seed(), ..c.clone() },
        obs_layout: OBS_LAYOUT.iter().map(|(n, s)| (n.to_string(), *s)).collect(),
        action_layout: Action::LAYOUT.iter().map(|(n, s)| (n.to_string(), *s)).collect(),
        hash_every: options.hash_every.max(1),
        record_obs: options.record_obs,
        initial_hash: env.state_hash(),
        determinism: DETERMINISM.into(),
    }
}

/// Writes a trajectory as it happens. Create it right after `reset`, call [`Recorder::record`] after each
/// step, then [`Recorder::finish`].
pub struct Recorder<W: Write> {
    out: W,
    options: RecorderOptions,
    steps: u64,
    /// Sim tick before the next step.
    tick: u64,
    total_reward: Vec<f32>,
}

impl<W: Write> Recorder<W> {
    /// Write the header for `env` (which must be freshly reset).
    pub fn new(mut out: W, env: &Env, options: RecorderOptions) -> io::Result<Self> {
        let options = RecorderOptions { hash_every: options.hash_every.max(1), ..options };
        write_line(&mut out, &Line::Header(header_for(env, options)))?;
        Ok(Self { out, options, steps: 0, tick: env.info().tick, total_reward: vec![0.; env.agent_slots().len()] })
    }

    /// Record one step: the observations the agents acted on, their actions, and the step's result. `env`
    /// is the env after the step.
    pub fn record(
        &mut self,
        env: &Env,
        obs_before: &[Obs],
        actions: &[KartInput],
        result: &StepResult,
    ) -> io::Result<()> {
        self.steps += 1;
        for (t, r) in self.total_reward.iter_mut().zip(&result.reward) {
            *t += r;
        }
        let last = result.terminated || result.truncated;
        let hash = (self.steps % self.options.hash_every as u64 == 0 || last).then(|| env.state_hash());
        let record = StepRecord {
            t: self.tick,
            obs: self.options.record_obs.then(|| obs_before.iter().map(Obs::to_vec).collect()),
            action: actions.iter().map(|a| Action::to_vec(&clamped(a))).collect(),
            reward: result.reward.clone(),
            events: result.events.clone(),
            hash,
        };
        self.tick = result.info.tick;
        write_line(&mut self.out, &Line::Step(record))
    }

    /// Write the footer (the env must be at the end of the episode) and give the writer back.
    pub fn finish(mut self, env: &Env, result: &StepResult) -> io::Result<W> {
        let footer = Footer {
            steps: self.steps,
            terminated: result.terminated,
            truncated: result.truncated,
            state_hash: env.state_hash(),
            total_reward: self.total_reward.clone(),
            report: env.report(),
        };
        write_line(&mut self.out, &Line::Footer(footer))?;
        self.out.flush()?;
        Ok(self.out)
    }
}

fn clamped(a: &KartInput) -> KartInput {
    Action::clamp(a).unwrap_or(*a)
}

fn write_line<W: Write>(out: &mut W, line: &Line) -> io::Result<()> {
    serde_json::to_writer(&mut *out, line).map_err(io::Error::other)?;
    out.write_all(b"\n")
}

/// A whole trajectory read back.
#[derive(Clone, Debug, PartialEq)]
pub struct Trajectory {
    pub header: Header,
    pub steps: Vec<StepRecord>,
    pub footer: Option<Footer>,
}

impl Trajectory {
    /// Read a trajectory file. Errors name the line.
    pub fn read(reader: impl BufRead) -> Result<Trajectory, String> {
        let mut header = None;
        let mut steps = Vec::new();
        let mut footer = None;
        for (n, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| format!("line {}: {e}", n + 1))?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Line>(&line).map_err(|e| format!("line {}: {e}", n + 1))? {
                Line::Header(h) if header.is_none() => header = Some(h),
                Line::Header(_) => return Err(format!("line {}: a second header", n + 1)),
                Line::Step(s) if header.is_some() && footer.is_none() => steps.push(s),
                Line::Step(_) => return Err(format!("line {}: a step outside header..footer", n + 1)),
                Line::Footer(f) if header.is_some() && footer.is_none() => footer = Some(f),
                Line::Footer(_) => return Err(format!("line {}: a footer without a header, or a second one", n + 1)),
            }
        }
        Ok(Trajectory { header: header.ok_or("the file has no header line")?, steps, footer })
    }

    pub fn read_file(path: impl AsRef<std::path::Path>) -> Result<Trajectory, String> {
        let file = std::fs::File::open(path.as_ref()).map_err(|e| format!("{}: {e}", path.as_ref().display()))?;
        Self::read(io::BufReader::new(file))
    }
}

/// Where a replay first stopped matching the recording.
#[derive(Clone, Debug, PartialEq)]
pub struct Divergence {
    /// 0-based step index (`steps.len()` for problems found in the footer).
    pub step: usize,
    /// Sim tick at which the recorded step started (the divergence is in or before this step's ticks).
    pub tick: u64,
    /// `fingerprint`, `initial_hash`, `action`, `hash`, `obs`, `reward`, `events`, `ended`, `footer_hash` or `report`.
    pub what: &'static str,
    pub detail: String,
}

impl std::fmt::Display for Divergence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} diverged at step {} (tick {}): {}", self.what, self.step, self.tick, self.detail)
    }
}

/// The outcome of [`replay`].
#[derive(Clone, Debug, PartialEq)]
pub struct ReplayResult {
    pub steps_replayed: usize,
    pub hashes_checked: usize,
    pub obs_checked: usize,
    pub first_divergence: Option<Divergence>,
    /// The final `state_hash` equals the footer's (false when there is no footer).
    pub final_hash_ok: bool,
    /// The final `RaceReport` equals the footer's.
    pub report_ok: bool,
}

impl ReplayResult {
    pub fn ok(&self) -> bool {
        self.first_divergence.is_none() && self.final_hash_ok && self.report_ok
    }
}

/// Observations and rewards go through decimal text, which can lose the last bit of an f32 once in a long
/// while; the state hash is the exact proof, these are checked to this tolerance.
const TOLERANCE: f32 = 1e-5;

fn close(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= TOLERANCE * (1. + x.abs().max(y.abs())))
}

/// Re-run a trajectory from its header's seed through a fresh [`Env`] and compare it with the recording:
/// the content fingerprint, the post-reset hash, every recorded `hash` and `obs` and `reward`, how the
/// episode ended, the final `state_hash` and the `RaceReport`. Stops at the first divergence.
pub fn replay(trajectory: &Trajectory) -> ReplayResult {
    let h = &trajectory.header;
    let mut result = ReplayResult {
        steps_replayed: 0,
        hashes_checked: 0,
        obs_checked: 0,
        first_divergence: None,
        final_hash_ok: false,
        report_ok: false,
    };
    let fail = |result: &mut ReplayResult, step: usize, tick: u64, what: &'static str, detail: String| {
        result.first_divergence = Some(Divergence { step, tick, what, detail });
    };
    if h.content_fingerprint != crate::content_fingerprint() {
        let detail = format!(
            "recorded {:#x}, this build {:#x}: the game's content differs",
            h.content_fingerprint,
            crate::content_fingerprint()
        );
        fail(&mut result, 0, 0, "fingerprint", detail);
        return result;
    }
    let mut env = match Env::new(EnvConfig { seed: h.seed, ..h.config.clone() }) {
        Ok(env) => env,
        Err(e) => {
            fail(&mut result, 0, 0, "action", format!("the header's config is refused: {e}"));
            return result;
        }
    };
    if env.state_hash() != h.initial_hash {
        let detail = format!("recorded {:#x}, replayed {:#x}", h.initial_hash, env.state_hash());
        fail(&mut result, 0, 0, "initial_hash", detail);
        return result;
    }
    let mut last: Option<StepResult> = None;
    for (i, rec) in trajectory.steps.iter().enumerate() {
        let tick = rec.t;
        if let Some(obs) = &rec.obs {
            let now: Vec<Vec<f32>> = env.observe().iter().map(Obs::to_vec).collect();
            if now.len() != obs.len() || !now.iter().zip(obs).all(|(a, b)| close(a, b)) {
                fail(&mut result, i, tick, "obs", "the observation the recorded action was chosen from differs".into());
                return result;
            }
            result.obs_checked += 1;
        }
        let actions: Result<Vec<KartInput>, EnvError> = rec.action.iter().map(|a| Action::from_vec(a)).collect();
        let step = actions.and_then(|a| env.step(&a));
        let step = match step {
            Ok(s) => s,
            Err(e) => {
                fail(&mut result, i, tick, "action", e.to_string());
                return result;
            }
        };
        result.steps_replayed += 1;
        if let Some(expected) = rec.hash {
            result.hashes_checked += 1;
            if env.state_hash() != expected {
                let k = h.hash_every as usize;
                let from = if k > 1 {
                    format!(" (first mismatching check; the step is within the last {k} steps)")
                } else {
                    String::new()
                };
                fail(
                    &mut result,
                    i,
                    tick,
                    "hash",
                    format!("recorded {expected:#x}, replayed {:#x}{from}", env.state_hash()),
                );
                return result;
            }
        }
        if !close(&step.reward, &rec.reward) {
            fail(&mut result, i, tick, "reward", format!("recorded {:?}, replayed {:?}", rec.reward, step.reward));
            return result;
        }
        if step.events != rec.events {
            fail(&mut result, i, tick, "events", "the step's events differ".into());
            return result;
        }
        last = Some(step);
    }
    let steps = trajectory.steps.len();
    let end_tick = trajectory.steps.last().map_or(0, |s| s.t);
    let Some(footer) = &trajectory.footer else {
        // Nothing to compare the end with; the per-step checks are all there is.
        result.final_hash_ok = false;
        return result;
    };
    let (terminated, truncated) = last.as_ref().map_or((false, false), |s| (s.terminated, s.truncated));
    if (terminated, truncated) != (footer.terminated, footer.truncated) {
        let detail = format!(
            "recorded terminated={} truncated={}, replayed terminated={terminated} truncated={truncated}",
            footer.terminated, footer.truncated
        );
        fail(&mut result, steps, end_tick, "ended", detail);
        return result;
    }
    result.final_hash_ok = env.state_hash() == footer.state_hash;
    if !result.final_hash_ok {
        let detail = format!("recorded {:#x}, replayed {:#x}", footer.state_hash, env.state_hash());
        fail(&mut result, steps, end_tick, "footer_hash", detail);
        return result;
    }
    result.report_ok = env.report() == footer.report;
    if !result.report_ok {
        fail(&mut result, steps, end_tick, "report", "the RaceReport differs".into());
    }
    result
}
