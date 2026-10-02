# The agent environment

`reset(seed)` / `step(actions)` over a Spooky Kart race, for anything that wants to drive a kart without a
window: learning agents, scripted tests, automated balance runs. It is a layer on top of the game's own
simulation (`src/env.rs`, `src/env/`, `src/policy.rs`, `src/recorder.rs`, the `spooky-kart-env` bin). It adds
no rules: a race through the env is byte-identical to the same race played by the window or a test (a test
compares the `state_hash` and the `RaceReport`). It needs no window, sound device, network or the `client`
feature. For what a generic engine interface would need, see [ENV_DESIGN.md](ENV_DESIGN.md).

## Use it

```rust
use spooky_kart::env::{Env, EnvConfig, Action};
use spooky_kart::policy::{by_name, rollout};

let mut env = Env::new(EnvConfig { agent_slots: vec![5], ..EnvConfig::default() })?;
let mut obs = env.reset(42);                          // Vec<Obs>, one per agent slot
loop {
    let actions = vec![Action::from_vec(&[1.0, 0.0, 0.0, 0.0])?];   // throttle, steer, drift, perk
    let step = env.step(&actions)?;                   // StepResult { obs, reward, terminated, truncated, events, info }
    if step.terminated || step.truncated { break; }
    obs = step.obs;                                   // obs[i].to_vec() is the 91-float vector
}
```

Command line (build without the window: `cargo build --no-default-features --bins`; add `--release` for speed):

```
spooky-kart-env rollout  --policy bot|random|idle|lookahead --seed N --difficulty easy|medium|hard
                         --agent-slot K[,K..] [--frame-skip N] [--max-ticks N] [--out FILE.jsonl]
                         [--no-obs] [--hash-every K] [--bot-difficulty D]
spooky-kart-env replay   FILE.jsonl           # exit 0 only if the whole file reproduces
spooky-kart-env validate [--seed N] [--difficulty D] [--agent-slot K[,K..]]
spooky-kart-env bench    [--seed N] [--ticks N] [--agent-slot K]
spooky-kart-env eval     --policy bot,lookahead --seeds 1..12 --agent-slot all [--difficulty D]
```

`cargo test --no-default-features` runs everything (about 3 s for the env tests).

## Configuration

`EnvConfig` (serde; a trajectory header carries it, so a replay rebuilds the same env):

| field | default | meaning |
|---|---|---|
| `seed` | 1 | what `Env::new` resets with; `reset(seed)` can use another |
| `grid` | the 8 characters in grid order | 1 to 8 characters; slot = index |
| `agent_slots` | `[0]` | karts driven by the caller (at least one, distinct). They are `Driver::Human` in the sim. Every other kart is one of the game's own bots, driven inside `Sim::step` at `difficulty` |
| `difficulty` | Medium | how the bots drive |
| `max_ticks` | 12,000 | truncate after this many ticks stepped since `reset`. A race is about 5,500 to 7,000 ticks |
| `frame_skip` | 1 | ticks per `step`, holding the action. The perk button is pressed on the first tick only (it is an edge) |
| `skip_countdown` | true | run the 240-tick countdown inside `reset`, so the first observation is at the green light |
| `reward` | see below | the weights |

Episode end: **terminated** when the sim says the race is over (every agent kart finished, or the game's own
rules ended it: the 30 s grace after the first finisher). **truncated** when `max_ticks` arrives first. After
either, `step` returns `EnvError::EpisodeOver` until `reset`. A finished agent's kart is put on the game's
autopilot and its later actions are ignored. `step` refuses (and changes nothing) a wrong number of actions
or a non-finite action value.

## Observation (`Obs`, 91 floats)

`Obs` is a separate struct from the sim: plain copied numbers, built only from what a driver could perceive.
`Obs::to_vec()` flattens it in this fixed order (`Obs::layout()`, `Obs::names()` and `Obs::bounds()` give the
names, one name per element, and the bounds). Angles are in units of pi radians and **positive is to the
right** of the kart's heading (the game's convention). Every element is clamped to its bound, so the bounds
are a guarantee.

| index | name | bounds | meaning |
|---|---|---|---|
| 0 | `speed` | 0..2 | ground speed / 30 m/s |
| 1 | `vel_forward` | -1..2 | velocity along the heading / 30 (negative = reversing) |
| 2 | `vel_right` | -1..1 | velocity across the heading / 30 (the slide) |
| 3 | `heading_error` | -1..1 | heading minus road direction, / pi (+ = pointing right of the road) |
| 4 | `lateral` | -1.6..1.6 | offset from the centreline / 9 m half width (+ = right; the wall stands at 1.56; beyond 1 is grass) |
| 5 | `lap_fraction` | -0.25..1.25 | how far round the current lap (slightly negative on the grid) |
| 6 | `lap_index` | 0..1 | laps completed / 2 |
| 7 | `offroad` | 0/1 | on the grass |
| 8 | `drifting` | 0/1 | |
| 9 | `drift_dir` | -1..1 | which way the drift leans (0 when not drifting) |
| 10 | `drift_charge` | 0..1.5 | charge / 2; 0.5 pays a tier-one boost on release, 1.0 tier-two |
| 11 | `boost` | 0..1 | boost time left / longest boost |
| 12 | `slowed` | 0/1 | slowed by a hazard or crows |
| 13 | `spin` | 0..1 | spin-out time left / full spin-out |
| 14 | `phased` | 0..1 | the Ghost's own phase time left |
| 15 | `perk_ready` | 0/1 | an active perk, recharged, not spinning: the button would work |
| 16 | `perk_cooldown` | 0..1 | recharge left / cooldown (0 for passive perks) |
| 17 | `countdown` | 0..1 | countdown left (0 once racing; only non-zero with `skip_countdown` off) |
| 18 | `rank` | 0..1 | race position (0 = first, 1 = last), as the HUD shows it |
| 19-26 | `character.<name>` | 0/1 | one-hot of the agent's own character (the public stats follow from it) |
| 27-34 | `lookahead_bearing.0..7` | -1..1 | bearing of the centreline point 5, 10, 15, 20, 30, 40, 55, 70 m ahead, from the heading, / pi |
| 35-42 | `lookahead_bend.0..7` | -1..1 | road direction at those distances minus the road direction here, / pi (+ = turns right) |
| 43-70 | `others.<0..3>.<field>` | | the 4 nearest other karts within 60 m, nearest first; 7 floats each (below) |
| 71-90 | `hazards.<0..3>.<field>` | | the 4 nearest hazards within 40 m, nearest first; 5 floats each (below) |

`others.i.*`: `present` (0/1), `distance`/60, `bearing`/pi, `rel_forward`/60 and `rel_right`/60 (their velocity
minus ours, in our frame, m/s), `arc_ahead`/60 (distance along the road, + = they are ahead), `rank_delta`/7
(their position minus ours; - = ahead in the order). All zeros when absent.
`hazards.i.*`: `present`, `distance`/40, `bearing`/pi, `radius`/3, `bone` (1 = bone, 0 = bandage).

**Observable** (a human could see or feel it): the agent's own kart (above), the public road ahead, the nearest
karts and hazards as seen from the seat, the race order the HUD shows.
**Not observable** (kept out of the type): other karts' perk cooldowns, drift charge, boost and slow timers,
bot skill and statistics counters; which character another kart is; hazard owner and remaining life; the RNG,
the exact race tick, world coordinates; karts beyond the nearest four or 60 m, hazards beyond four or 40 m;
other agents' observations. `validate` proves hidden state cannot move the observation by changing it and
checking.

`StepResult.info` (`ranks`, `finished`, ticks) is privileged ground truth for logging, not for a policy.

## Action

`KartInput { throttle, steer, drift, perk }` or four floats via `Action::from_vec` / `Action::to_vec`:

| index | name | range | |
|---|---|---|---|
| 0 | `throttle` | -1..1 | gas (+), brake or reverse (-) |
| 1 | `steer` | -1..1 | left (-) to right (+) |
| 2 | `drift` | 0..1 | held when >= 0.5 |
| 3 | `perk` | 0..1 | pressed when >= 0.5 (first tick of the step only) |

Out-of-range values are clamped. NaN and infinity are refused with an error, never turned into a number (a NaN
steer would poison the sim, since the sim's own `clamp` passes NaN through).

## Reward

Per agent, per step, a pure function `env::reward(cfg, prev, next, events)` of the agent's counters before and
after the step's ticks (`Kart.progress`, `stats.wall_hits`, `stats.offroad_ticks`) and the `Finished` event:

```
r = 0.01 * (progress metres this step)          forward positive, backward negative: the primary signal
  - 0.5  * (wall hits this step)
  - 0.002 * (ticks on the grass this step)
  + [2 + 8 * (racers - place) / (racers - 1)]   once, on the step the kart crosses the line
r = 0 once the kart had already finished
```

A clean race (two 900 m laps) is worth about 18 from progress; finishing pays 2 (last) to 10 (first); a wall
hit costs two seconds of top-speed progress. Not finishing pays no bonus. Weights are `RewardConfig` in the
config. The reference bot's total over a race is about 22 to 27.

## Trajectory file (JSON lines) and replay

One JSON object a line, `"type"` first:

1. `{"type":"header", "game":"spooky-kart", "version", "content_fingerprint", "seed", "difficulty", "grid",
   "agent_slots", "frame_skip", "max_ticks", "skip_countdown", "config": {...all of EnvConfig...},
   "obs_layout", "action_layout", "hash_every", "record_obs", "initial_hash",
   "determinism": "per machine; same binary and platform reproduce exactly"}`
2. one `{"type":"step", "t", "obs", "action", "reward", "events", "hash"}` per `Env::step`: `t` is the sim tick
   the observation was taken at (240 for the first step, because the skipped countdown is tick 0 to 240);
   `obs` is what each agent acted on (omitted with `--no-obs`); `action` is the clamped action per agent as
   four floats; `reward` per agent; `events` all sim events of the step's ticks (tagged `type`); `hash` is the
   sim `state_hash` after the step, every `hash_every` steps and always on the last.
3. `{"type":"footer", "steps", "terminated", "truncated", "state_hash", "total_reward", "report": RaceReport}`.

`replay FILE` (or `recorder::replay`) rebuilds the env from the header, refuses a file whose
`content_fingerprint` is not this build's, feeds the recorded actions, and checks the post-reset hash, every
recorded hash, observation (to 1e-5), reward and event list, how the episode ended, the final hash and the
`RaceReport`. It stops at the first divergence and names the step, the tick and what differed. Evidence from
this branch: a 5,966-step bot race (5.4 MB with observations) replays in 0.13 s dev profile with all 5,966
hashes and observations matching; a frame-skip-4, hash-every-10, no-obs race is 0.34 MB and replays the same
way; tests tamper with an action, an observation, the footer and the fingerprint and check each is caught at
the right step. A trajectory is about 0.9 KB a step with observations (frame_skip 1), 0.24 KB without.

**Promise**: per machine. The same binary on the same platform reproduces a trajectory exactly. f32 `sin`/`cos`
may differ across platforms or compilers (see DESIGN.md), so a file recorded on Windows is not promised to
replay on Linux. u64 hashes are JSON integers above 2^53: a JavaScript reader needs BigInt. f32 values round
trip through the JSON text bit for bit (a test scans 400,000 floats).

## Policies and results

`bot` is the game's own AI (`bot::drive_as`) run through the env. **It reads the whole sim and is omniscient**:
the privileged reference, not an example of what an observation can do. `lookahead` is the one policy that
uses only the observation (its type, `ObsPolicy`, can only be called with an `Obs`): aim at the centreline a
little ahead (interpolating the lookahead bearings), slow for the bend ahead using the Medium bot's numbers,
no drifting, no perks. `idle` and `random` are the floor.

Measured with `eval --agent-slot all --seeds 1..12` (each of the eight slots on its own, 96 races a row, bot
and baseline both at Medium driving, rivals at the difficulty shown; mean finishing place, mean finish time):

| rivals | bot (privileged) | lookahead (obs only) | random | idle |
|---|---|---|---|---|
| Easy | 1.00, 94.7 s | 1.52, 96.3 s | 8.00, DNF | 8.00, DNF |
| Medium | 4.44, 96.9 s | 4.73, 96.7 s | 8.00, DNF | 8.00, DNF |
| Hard | 5.56, 97.8 s | 5.73, 97.3 s | 8.00, DNF | 8.00, DNF |

Read it honestly: the obs-only baseline is the Medium bot re-derived from the observation without drift and
perks, so it lands within about 0.2 to 0.5 of a place of the privileged bot (and at Medium and Hard rivals is
as fast or faster on the clock). That shows the observation carries what a competent driver needs; it does not
show that a learned policy will. Random and idle never finish (they last by the game's grace rule). The
grid is not fair across characters, so a single slot's place says more about the character than the policy:
use `--agent-slot all`. Bot at Hard against Hard rivals: 4.66, 95.2 s.

## Speed

Headless, `spooky-kart-env bench --seed 1 --ticks 300000 --agent-slot 5`; ticks are 60 Hz sim ticks. The
machine is a shared 4-core Intel N97 (other jobs were running: load average 4 to 8), so treat the dev numbers
as noisy and quote ranges.

| profile | `Sim::step` alone (8 bots, no env) | env + `bot` | env + `lookahead` |
|---|---|---|---|
| release (opt 3, thin LTO) | 228,000 to 230,000 ticks/s | 172,000 to 174,000 ticks/s (2,900x real time) | 176,000 to 178,000 ticks/s |
| dev (game opt-level 2, debug assertions) | 20,000 to 72,000 ticks/s | 20,000 to 60,000 ticks/s | 25,000 to 62,000 ticks/s |

A 5,900-tick race takes about 34 ms release (about 29 races/s; the release figure was steady across the last runs even under load) and 100 to 300 ms dev depending on load, on one core. The env costs
about 25% over the bare sim (building the observation: about 1.4 us a tick); `frame_skip` amortises it. A JSON
encode and decode of each step's observation and action (what a stdio bridge would do) measured 15 us a step
including the step itself, 1,070 bytes a step, before any pipe cost.

## What is not provided

- **Pixels.** No rendering path: the window needs the `client` feature, and a software-rendered race takes
  about seven minutes (STATUS.md), about a thousand times slower than the sim.
- **Audio.** Sim events are reported (wall hits, bumps, perks), positionless; no sound is synthesised.
- **A Python (or any other language) binding or a bridge process.** Run the bin and read its JSON lines, or
  link the crate. A stdio JSON-lines stepping bridge is the likely next step (ENV_DESIGN.md).
- Vectorised or parallel envs (run several processes), a Gym-compatible registry, saving and restoring
  mid-episode (the sim supports saves; the env does not expose them).
- Cross-platform replay.
