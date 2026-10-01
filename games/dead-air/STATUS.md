# Dead Air Status

## Completed

A complete, playable first version: a flashlight-and-pistol survival horror game on BlueEngine's
custom-sim road.

- **The shift loop**: keep the Hollow Pine relay station's generator fuelled, send five scheduled
  all-clear broadcasts before the 00:00-06:00 shift ends, survive. Winning and losing (caught, or ran
  out of time) are both implemented and tested.
- **The Caller**: a blind, sound-hunting monster with a real state machine (Patrol / Investigate / Hunt /
  Staggered) that paths through a 12-node waypoint graph matching the station's layout. It is drawn as a
  deliberately featureless pale silhouette with no glow, so heavy fog and low ambient light are what hide
  or reveal it, not a special-case render path.
- **Flashlight and pistol**: both only ever stagger the Caller for a few seconds (a cone-and-range check
  for light, a hitscan ray for the pistol); neither kills it. Ammo (4 rounds, +2 from a pickup) and
  flashlight battery (drains only while on, 2 battery packs in the world) are both scarce on purpose.
  Sneaking (held crouch) lowers movement noise.
- **Generated audio**: every sound is computed, not recorded (`devkit::synth`). A dark ambient loop keyed
  to this game's own title/tagline via `ambient_spec_for` plays as background music; three sounds
  (gunshot, the Caller's stagger shriek, a proximity heartbeat that quickens on a hunt) are hand-built
  from the same DSP primitives; the rest reuse existing presets.
- **Persistent audio settings**: the pause menu's Settings screen (`GameShell::local_menu_with_audio`)
  has Music/Sound toggles and a "Save music (.wav)" button that writes the generated track to the
  player's Downloads folder.
- **Story**: five station-log pages scattered through the building, readable any time via the in-game
  Journal (J), tell the previous operator's story in her own words without a single cutscene.
- **Tests**: 17 inline unit tests (`src/lib.rs`) covering every mechanic (generator drain/refuel,
  flashlight battery, pickups, broadcasts/win, time-out loss, noise drawing the Caller, pistol/flashlight
  stagger, catch, dry fire) plus determinism; 7 black-box integration tests (`tests/determinism.rs`)
  covering determinism, save/load round-trips (including a damaged save and a quick-save slot), and the
  public win/lose flow. `python scripts/check.py --skip-ship` passes.
- Identity, generated icon, packaging and desktop-shortcut tooling (from the scaffold), not yet shipped.

## Next Steps

- Ship it: fill in any remaining identity details if the title/tagline change, then `scripts/blue ship`
  (needs a real desktop; not run from this headless session).
- Look at real frames with a human in the loop and actually play it: headless capture confirmed the
  rendering pipeline, lighting and the Caller's silhouette work (see the fix history in git log for the
  render-order and lighting bugs those captures caught), but nobody has played a full shift by hand yet.
- Possible next layer, not started: more station rooms, a second monster behavior for very late in the
  shift, more station-log pages, a title/options screen before the first spawn.
