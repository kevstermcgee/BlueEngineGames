# Design note: what a generic engine agent interface needs

Written after building the Spooky Kart env ([ENV.md](ENV.md)) as the first real consumer, to be the proposal for
promoting an interface into BlueEngine's `devkit` (slice 2). Everything here is a lesson from this build, not
a guess; where a number is quoted it was measured on this branch. Size of the build, for scale: about 2,400
lines of code (`env.rs` 620 with tests, `obs.rs` 445, `validate.rs` 366, `recorder.rs` 392, `policy.rs` 217,
the bin 327) and 380 lines of tests.

## (a) The minimal trait surface, and what was game-specific

What an agent needs is five things: `reset(seed)`, `step(actions)`, an observation, a reward, and a done signal
(split into terminated and truncated). In this build:

| part | generic or game-specific | notes |
|---|---|---|
| the loop: validate and clamp actions, hold them `frame_skip` ticks, count ticks, truncate at `max_ticks`, refuse stepping after the end, build `StepResult` | **generic** (about 100 lines of `Env::step`/`reset`) | only needs `Simulation::step`, a way to put an action into the game's `Input`, and "is it over" |
| `state_hash`, determinism, reset reproducibility, frame-skip equivalence, truncation, finite/in-bounds observations, step-after-end refusal (7 of the 9 `validate` checks) | **generic** | need only the loop plus the observation spec |
| recorder and replay (header, per-step hash, footer, first-divergence report) | **generic**, given a serialisable action vector, a content fingerprint, a final report | the footer's `RaceReport` was the only game type in it |
| reading `Env::step` results, `Action` as a float vector with a layout and bounds | **generic** | the spec (names, sizes, low, high) is data |
| what the agent perceives (`Obs`: lookahead, nearest karts and hazards, own kinematics) | **game-specific**, all of it (445 lines) | no part of it generalises; only the *conventions* do (relative, normalised, clamped, named groups) |
| the reward (progress delta, wall hits, grass, finish place) | **game-specific** (about 30 lines plus weights) | the *shape* (a pure function of counters before and after plus events) is reusable |
| the terminated rule | **game-specific** but one line (`sim.is_over()`); the race's own rule already existed | |
| who the agents are (`agent_slots` become `Driver::Human`, the rest stay on the game's bots) | game-specific, **but it is the seat model every `NetGame` already has** | see (c) |
| the reference policy (`bot::drive_as`) | game-specific | it existed already; wrapping it was 10 lines |

So the generic part is the loop, the validator and the recorder; a game supplies four small functions. A
sketch of the companion trait (not built; the names are suggestions):

```rust
trait AgentGame: Simulation {
    fn spec(&self) -> AgentSpec;                       // obs layout + bounds, action layout + bounds, agents
    fn input(&self, agent: usize, action: &[f32]) -> Self::Input;   // or merge into one Input for the tick
    fn observe(&self, agent: usize) -> Vec<f32>;       // the only door to what the agent sees
    fn reward(&self, agent: usize, before: &Self::Status, events: &[Self::Event]) -> f32;
    fn done(&self) -> bool;                            // terminated; truncation is the env's
    fn fingerprint(&self) -> u64;                      // content version for trajectory headers
}
```

and a generic `Env<G: AgentGame>`. Things the sketch must cover that only showed up while building:
the countdown (`skip_countdown`: a game may need a "settle" phase before the first observation), per-agent
done (here one `terminated` for all agents; a finished agent is put on autopilot by the sim), and
`frame_skip` semantics for edge-triggered buttons (the perk is a press, so it is applied on the first tick only;
a generic spec must say per action element whether it is a level or an edge).

## (b) Observation versus privileged state

**What enforced the separation here**, in order of strength:

1. **The type.** `Obs` holds plain copied numbers and no reference to `Sim`. The policy never receives a sim.
   `ObsPolicy::act(&mut self, &Obs)` cannot reach anything else, so the obs-only baseline is *unable* to cheat,
   and the omniscient reference is a different trait (`Policy::act(&Env, ..)`) that says so
   (`privileged() == true`).
2. **One door.** Everything is built in `Obs::observe`; there is no other path from sim to agent. Everything is
   relative (to the kart, to the road): no world coordinates, so a policy cannot memorise the map by position.
3. **A mutation test.** `validate` changes every hidden thing it can think of (other karts' perk cooldowns,
   drift charge, boost, skill, counters; hazard owners and lifetimes) and checks the observation is unchanged,
   then moves a visible kart as a positive control to prove the check can see anything.

**What is not enforced**: `Obs::observe` takes `&Sim`, so the compiler only constrains its *output*; a leak would
be one careless line in one function. The mutation test only covers fields someone listed. `Env::sim()` is
public (the reference bot needs it; so would any privileged critic). Visibility is also a design choice the
types cannot make: is a rival's perk cooldown visible? (Here no; the HUD does not show it. The race order *is*
visible.) The bounds are a clamp, so an out-of-range value is hidden, not detected.

**What a generic version needs**: (1) `observe(&self, agent) -> Obs` as the only door and `Obs` a distinct
serialisable type; (2) a separate, explicitly named `privileged_state()` for centralised training (PettingZoo's
`state()`), never mixed into observations; (3) the bridge (e) carries only `Obs`, which turns "cannot reach the
sim" from a convention into a process boundary; (4) a generic invariance check where the game supplies a
`perturb_hidden(&mut Sim, agent)` hook (the thing I wrote by hand in `validate`), ideally generated from a
declared list of hidden fields so adding a field forces a decision; (5) the spec carries a one-line
"observable / not observable" statement per group, because that is what the next author needs to read.

## (c) Action encoding, and whether `Simulation::Input` needs serde

The env needs to record actions and replay them. The engine's `Simulation::Input` is only
`Clone + Default`; a serde bound on it would break every implementer for a feature most never use.

**Recommendation: a new optional companion trait, not a change to `Simulation`.** What worked here: the action
the env records is a **float vector with a spec**, not the game's `Input`. `Action::from_vec` turns four floats
into a `KartInput` (clamping, thresholding the two booleans, refusing NaN); `Action::to_vec` is the inverse.
Replay re-runs `from_vec` on the recorded vector, so the file format never depends on how `Inputs` is laid out,
and `Inputs` needed no serde (`KartInput` happens to have it; `Inputs([KartInput; 8])` has it too, but the env
does not use it). The spec generic code needs per element: name, low, high, and whether it is continuous,
boolean (threshold 0.5) or an edge. A discrete `Discrete(n)` kind will be needed for games with menus of
actions; Spooky Kart has none.

Other things learned about actions: clamping must be done by the env *before* the sim (`f32::clamp` passes NaN
through, so a NaN steer would have poisoned the race: the env refuses non-finite values with an error instead,
which is why `step` returns a `Result`); record the *clamped* action, which is what the sim saw.

On seats: the engine's `NetGame::step(&[Option<Input>])` already has the right shape (`None` = the game's own
AI). Here the seat decision is made at construction (`agent_slots` become `Driver::Human`) because that is how
Spooky Kart's sim distinguishes humans from bots; a generic env should take seat assignment at `reset`, as
netplay does, rather than per step.

## (d) Determinism promise and the trajectory and replay format

**Promise: per machine.** The same binary on the same platform reproduces an episode exactly. It must not claim
more: Spooky Kart's DESIGN.md already says f32 trig may differ across platforms, which is why its multiplayer is
server-authoritative, not lockstep. A cross-platform promise would need fixed-point or a software libm in the
sim, a game decision, not an env one. The promise is written in every trajectory header.

**What proves it**: `Simulation::state_hash` after every step (it was cheap enough: a 5,966-step race is
5.4 MB with observations, replays in 0.13 s), plus the final hash and the full `RaceReport` in the footer. A
replay reports the *first* divergence: step, tick and what differed (fingerprint, post-reset hash, a step hash,
an observation, a reward, events, how it ended, the footer hash, the report). With `hash_every = K` the tick is
only localised to K steps, so the recorded observations and rewards (checked every step when present) narrow it.

**Format (JSON lines)**: header, step records, footer. Header fields a generic format needs: game id, crate
version, **content fingerprint** (a replay is refused if the content differs: here `content_fingerprint()`, the
number the netplay handshake already uses), the seed, the whole config (so reward weights and seats are part of
the file), the obs and action layouts, hash cadence, the determinism sentence, and the hash right after `reset`
(catches reset drift before any step). Practical notes: (1) observations are optional in the file (replay does
not need them, a learner does): 0.9 KB a step with them, 0.24 KB without; (2) u64 hashes exceed JavaScript's
2^53, so a JS reader needs BigInt; (3) f32 values must survive the text round trip bit for bit or replays would
drift: serde_json's default float parsing passed a scan of 400,000 floats, but it does not *promise* this, so
either enable its `float_roundtrip` feature in the generic version or store action bits; (4) a binary format
is a later optimisation; (5) the engine's `Snapshot` (this game is `SavePolicy::Exact`) would let a trajectory
start mid-episode from a save instead of a seed, and give agents cheap forking for tree search (`Sim` is not
`Clone`). I did not build that.

A cost I met: `sim::Event` is not `Serialize`, so the env defines its own mirror enum. A generic recorder
wants the game's event type to be serialisable (another bound that belongs on the companion trait, not on
`Simulation`).

## (e) The cheapest external bridge

Options for a process that is not Rust (Python above all):

| option | cost | verdict |
|---|---|---|
| **stdio JSON lines**: `spooky-kart-env serve` reads `{"cmd":"reset","seed":1}` and `{"cmd":"step","actions":[[..]]}` lines, writes `{"obs":..,"reward":..,"terminated":..}` | about 150 lines; no new dependency (serde_json is already here); reuses `EnvConfig`, the obs and action vectors, the replay format; Python needs only the standard library | **recommended** |
| netplay as the protocol | the QUIC/UDP wire codec, lobby and join key reimplemented in Python; and the server runs at 60 Hz wall-clock, so a 100 s race takes 100 s (versus 34 ms) | no: netplay is for playing real-time with humans, not for stepping faster than real time; its snapshots are server-authoritative state, not an observation |
| PyO3 or a C ABI | a build and packaging burden (especially Windows), and a new dependency | later, if throughput needs it |
| shared memory or a socket protocol | more work, more platform surface | only if JSON throughput is the measured limit |

**Recommendation: stdio JSON lines**, one process per env (vectorise by running several; a `step_many` command
is a small extension). It makes the privileged/observation boundary a process boundary: only `Obs` crosses.
Cost measured: encoding and decoding each step's observation and action as JSON took 15 us a step including
the step itself (1,070 bytes a step), so the bridge itself is not the bottleneck; the pipe round trip and the
Python parse will be (my estimate, not measured: roughly 10 to 30 thousand steps/s per process at
`frame_skip` 1, against 173,000 ticks/s raw). `frame_skip` 4 makes that comfortable. Not built in this slice,
by agreement.

## (f) What a sensory-model layer could attach to, and what is missing

The seam is `Obs::observe`: a sensor is another function from `&Sim` and a slot to floats, appended as a named
group in `LAYOUT` (the layout, names and bounds machinery already handles groups).

| sense | what the game offers | what is missing |
|---|---|---|
| **spatial rays** | everything that matters is 2D geometry the sim already has: the wall is an offset of the centreline (`Track::nearest`, wall at 14 m), karts are discs (radius 1.2), hazards are discs. A ray fan is a short function over those | no generic raycast API in the game or engine to call; Deadfall's line-of-sight fans (per the plan) are the thing to promote. No scenery occlusion exists in the sim (scenery is visual only), which is correct for a kart racer but means "what a ray sees" and "what the camera sees" differ |
| **audio events** | the sim already emits the cue vocabulary the window plays (`WallHit`, `Bump`, `PerkUsed`, `HazardHit`, `DriftBoost`, `LapDone`) | events carry kart ids, not positions or loudness, so "heard from the agent's seat" needs a position and attenuation model (additive: positions are in the sim at that tick); no engine-noise or tyre-slip sound exists, and `music.rs` is presentation |
| **pixels** | a software-rendered race takes about 7 minutes (STATUS.md), roughly 1,000 times slower than the sim; the render path needs the `client` feature | an offscreen, fixed-camera render path with a GPU or at least a fast CPU path; without it pixels are for evaluation snapshots, not training |

So the cheapest honest next sense is a 2D ray fan (a few dozen lines, no dependency); audio is a modest
additive change to events; pixels are a separate engine project.

## (g) Open decisions for Kevin

1. **When to promote into `devkit`.** I recommend after a second game (Dead Air, with a different observation
   and action shape) has used the same loop, so the trait is shaped by two consumers. The generic pieces
   (loop, validator, recorder) could move earlier if you would rather not wait.
2. **Determinism level.** Per machine (done) or cross-platform (needs a sim change, and a decision about
   fixed-point or a libm). I recommend per machine for now.
3. **Seats**: per-reset assignment (netplay's model, recommended) or `agent_slots` fixed in config as now.
4. **Reward ownership.** Always supplied by the game (as here), or also a generic declarative reward for
   `GameDocument` games (derived from `complete`/`fail`, as the study sketched)?
5. **Bridge now?** Build the stdio JSON-lines server as slice 3 (recommended), or stay in-process only.
6. **Observation breadth.** Spooky Kart's choices are mine: nearest four karts within 60 m, rival characters not
   shown, rank shown. These are game-design questions (what should a fair driver know?) that will matter once a
   learned policy competes with people online.
7. **Online fairness.** Do agents ever join the hosted server? Online races are fixed at Medium bots and
   server-authoritative; an agent client would be a bot client with a policy. Not built, not needed yet.
8. **Whether the env bin ships** in the published download. It is declared like the server and bots bins (no
   `required-features`), so it builds with the game; `ship.py` prefers the package-named bin so the package is
   unaffected, but the published copy may not want it.
9. **Multi-agent semantics**: one `terminated` for the episode (as now) versus per-agent done flags
   (`info.finished` has them for logging).
