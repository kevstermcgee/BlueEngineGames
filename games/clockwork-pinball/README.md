# Clockwork Pinball

A neon pinball table run by real rigid-body physics: a steel ball with continuous collision detection, kinematic flippers and a spinning gear, bouncy walls, bumpers, a three-lane multiplier and a spring plunger.

## Physics

The playfield is flat in the physics world and tilted by a gravity vector that points partly down the table. The ball is a CCD sphere; flippers and the gear are kinematic bodies moved by pose each tick (the solver derives their velocity from the motion, which is what makes a flick launch the ball).

## Play

```
cargo run --release
```

A/D or arrows (or triggers/bumpers) flip, hold Space (A button) and release to plunge, R restarts, F5/F9 save/load. No assets are needed: every model and sound is synthesised by the engine's kit.

For agents: `--capture DIR --frames 30,90 --autoplay` saves screenshots, `--script` drives the human input path, and
`python tools/xcapture.py` (in BlueEngine) runs it on a machine without a display.

## Tests

14 tests: determinism, incline, tunnelling, plunger, flipper, bumper, lanes, drain, autoplay keeps scoring, save/resume drift, bad saves refused. The rules are a pure library (`src/lib.rs`), so all of it runs without a window: `cargo test`.

## Developer feedback

What building this on BlueEngine was like (see also `docs/AI_DEV_FEEDBACK.md` in the engine):

- **Fastest path worked:** `custom-sim` scaffold -> pure `Sim` + tests -> window. The simulation was tested (ball rolls down the incline, flipper launches, tunnelling) before anything was drawn.
- **Biggest gap: no physics for custom sims.** The engine's prop world (HeadlessWorld/SceneBuilder) is first-person oriented: no runtime spawn, no kinematic bodies, no restitution control. I wrote `src/physics.rs` (~370 lines) over raw `vesper3d::rapier`: materials, stable body slots, kinematic moves, states/restore, hashing. Every physics game will need exactly this.
- **Conventions cost rounds:** rapier yaw maps +X to (cos, 0, -sin), friction combines as the average of both surfaces, kinematic bodies report zero velocity unless moved by pose, and small fast balls need CCD. None of it is written down in the engine docs.
- **Saves:** `SavePolicy::PhysicsContinuation` plus `assert_loads_replay_identically` caught a real bug (restore reused the old world's contact cache). The recipe 'rebuild the world, then apply saved poses' deserves a doc page.
- **Tuning by bot:** a 30-line `autoplay` bot (flick every 26 ticks) gave objective balance numbers (score after N ticks) without a human, and found a dead-end on the table.
- **Headless visuals:** `tools/xcapture.py` let me judge camera and colours from frames; neon look from `Look::night()` plus emissive boxes needed no assets.
