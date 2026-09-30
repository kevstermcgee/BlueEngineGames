# Wobble Tower

Stack swinging crates on a small platform under real physics and rising wind. Heavy crates, light crates, friction, tipping and gusts decide how high you get; three dropped crates end the run.

## Physics

Crates are dynamic boxes constrained to a vertical plane (locked z translation and x/y rotation), with per-kind density and friction and CCD. A crane drops them; a crate counts as stacked when it has settled. Wind is a horizontal acceleration that grows with tower height.

## Play

```
cargo run --release
```

Space / A button drops the crate, R restarts, F5/F9 save/load. No assets are needed: every model and sound is synthesised by the engine's kit.

For agents: `--capture DIR --frames 30,90 --autoplay` saves screenshots, `--script` drives the human input path, and
`python tools/xcapture.py` (in BlueEngine) runs it on a machine without a display.

## Tests

13 tests: landing and settle, modest tower stands, wind topples a tall tower, overhang tips off, lives, gusts, heavy-vs-light friction on a slope, planar constraint, saves. The rules are a pure library (`src/lib.rs`), so all of it runs without a window: `cargo test`.

## Developer feedback

What building this on BlueEngine was like (see also `docs/AI_DEV_FEEDBACK.md` in the engine):

- **Stacking is the stress test for a physics layer:** resting contact, sleeping and friction all show up. With 8 solver iterations at 60 Hz the towers are stable (the 'stands 10 s' test pins that). The engine has no stacking or settle test to copy; I wrote the 'tower stands 10 s' test and it was the most useful one.
- **Planar bodies** (2D physics in a 3D engine) needed axis locks; I added `dynamic_box_planar` to my layer. The engine should offer it, since most 'mini games' are really 2D physics.
- **Determinism held** across restarts of the world because I rebuilt it from scratch; bodies are keyed by stable slots, so dropped and removed crates do not shift the ids.
- **Bot as balance oracle:** `balance_point()` (mean x of the top three settled crates) doubled as the bot's target and the HUD hint; the rule code was reusable in three places because it was a pure library.
- **Rendering:** crates of several sizes needed a Template per (kind, size); a HashMap keyed by quantised size worked. `Fx::beam` made wind streaks cheap. No asset pipeline was needed.
- **Friction:** heavy vs light crates only looked different once the floor friction test made the difference measurable (floor friction 0.1, push 4 m/s^2).
