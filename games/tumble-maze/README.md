# Tumble Maze

Tilt a wooden board to roll a steel marble through three mazes: open sinkholes that swallow it (+5 s), stars to collect, and sweeping bars that shove it around. Par times, stars and a clock decide the score.

## Physics

The board is flat in the physics world and tilted by rotating the gravity vector (a ball on a real incline rolls identically). The marble is a rolling CCD sphere, walls are fixed boxes merged into runs so it never snags on a seam, and the sweeping bars are kinematic bodies that shove it.

## Play

```
cargo run --release
```

WASD / arrows / left stick tilt the board, R restarts, F5/F9 save/load. No assets are needed: every model and sound is synthesised by the engine's kit.

For agents: `--capture DIR --frames 30,90 --autoplay` saves screenshots, `--script` drives the human input path, and
`python tools/xcapture.py` (in BlueEngine) runs it on a machine without a display.

## Tests

14 tests: determinism, every level solvable by search, every star reachable, marble accelerates at 5/7 g sin(a), coasts to rest, cannot leave the board, hole penalty, no tunnelling, stars once, bars move, the bot beats every level by rolling, saves. The rules are a pure library (`src/lib.rs`), so all of it runs without a window: `cargo test`.

## Developer feedback

What building this on BlueEngine was like (see also `docs/AI_DEV_FEEDBACK.md` in the engine):

- **Third time was the cheapest.** With `physics.rs` copied from the earlier games, the simulation, 14 tests and the window compiled and passed with no physics debugging at all; the only real work was level design. That is the argument for promoting the layer into the engine.
- **ASCII levels make solvability a test:** breadth-first search proves every level and star reachable, and the same search drives the bot, which then proves each level can be *rolled*, not just walked (it found no falls on the final levels, so the holes are fair).
- **A physics claim is testable:** the marble's acceleration down an incline matches 5/7 g sin(a) within 15% (rapier's sphere inertia is right), which gives confidence in every other number.
- **Analog input:** `Lifecycle<Held>` takes any `Copy` struct, so stick tilt stayed analog while `--script` cues map to -1/0/1. The `GamepadFrame` stick Y sign (positive up) is the trap.
- **Seams:** merging horizontal wall runs into single boxes removed ball-snagging; a general 'merge static boxes' helper would help.
- **Camera:** a near wall hides the near row when the camera is low; framing needs a pitch that depends on board size. A `View::frame_rect` helper would save the trial and error.
