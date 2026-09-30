# Development evidence and engine feedback

The first implementation uses the supported custom-sim starter, fixed tick Lifecycle, Controller, Snapshot, batched kit geometry and materials. Automated tests pass for exact deterministic replay/save continuation, cork floating versus steel sinking, fire heating/fuel consumption, droplet quenching and bounded spawn populations.

Initial real-frame review (`evidence/initial`) caught an upside-down mirror texture. Correcting render-target V coordinates fixes it. The mirror additionally needs an explicit depth attachment: default Macroquad render targets otherwise cannot resolve overlapping geometry. An off-axis aperture projection is required for correct parallax; simply reflecting yaw and using the player's ordinary perspective projects the wrong region into the mirror. These are substantial renderer plumbing tasks for a small experiment.

Largest engine obstacles, ordered by actual implementation cost:

1. **Planar mirrors lack a kit abstraction.** The game must construct the reflected camera, off-axis projection, target/depth setup, Camera trait adapter and reversed texture coordinates itself. Improvement: provide a tested planar mirror renderer in kit with explicit dimensions, front-side validation and target lifetime. Keep recursion and clipping constraints documented.
2. **Emissive fire does not illuminate nearby surfaces.** Kit supports hemisphere/key/rim lighting but has no localized dynamic lights. Improvement: bounded point lights with finite-radius attenuation, default-off behavior and stable shader uniform contracts. Fire flicker and a moving lab light should exercise it in this game.
3. **Environmental interaction is game-owned.** No public fluid/thermal solver was discovered; this game implements sphere contacts, approximate buoyancy, heat and droplet cooling explicitly. A full fluid/PBR solver would be a large separate engine project. Document this limitation rather than presenting illustrative water as validated continuum physics.
4. **Stale native tooling can generate stale scaffolds.** The prebuilt tools executable generated a launcher using `python` and without executable permissions; current source already discovers Python 3. The local launcher is repaired. Use freshly built native tools when comparing engine contracts.

The game deliberately reports its simplified model in the HUD and README. Glass and ice currently vary mechanical properties and color, without fracture, refraction or melting. There are no dynamic shadow maps. Final engine fixes and validation results are appended below.

## Implemented engine improvements

- `kit::MirrorPlane` validates the physical aperture; `kit::PlanarMirror` owns a bounded depth-backed render target, off-axis reflection camera, near-plane clipping and upright surface UVs. The lab deleted its handwritten Camera adapter and projection/texture setup and uses the engine API. Tests check off-center and rotated apertures, invalid/back-side views and clipping behind the plane.
- `kit::PointLight` and `Materials::set_point_lights` provide four finite-radius unshadowed diffuse lights per pass. `set_scene` clears them for existing-client compatibility. The lab now uses fire flicker, burning-sample lights and a moving blue light. Tests check attenuation, direction, bounds/invalid inputs and the complete shader uniform contract.
- Public contracts are documented in BlueEngine’s CUSTOM_CLIENT and CUSTOM_SIM_CHEATSHEET and discoverable through its feature index. No game-specific simulation rules moved into the renderer.

## Verification

- Game project checks pass, including material/buoyancy, heat/quenching, input toggles, population limits, deterministic replay and exact save continuation.
- Real Linux X11 keyboard events exercised M/E/1/2/L/G/B, WASD, jump, F5, reset, F9 and Escape against the shipped executable, independently of unattended/scripted input. Captures in `evidence/controls` show changed state, reset, restored toggles/material/population and the pause menu.
- `evidence/shipped` and `evidence/engine` captures were visually inspected for upright reflection, sideways parallax, water flow, fire, the material lane and lighting. Final shipping smoke frames live under `.blue-check/smoke-*`; the icon, package stamp and desktop shortcut pass the ship gate.
- Linux live-window/shortcut launch introspection and Windows executable-resource checks are skipped by the shipping tool on Linux. Windows and gamepad input were not exercised.
- The first engine full run failed at the sandbox-denied UDP localhost test, after 362 other library tests passed. The full run was repeated with socket access. Temporary validation files use the workspace because the machine’s `/tmp` filesystem is nearly full.
- Published-game export validation passes (254 files). No publication or git commits were requested or performed.

Final full BlueEngine check: **PASS**, all 10 commands in 177.446 seconds. Report: `/home/kevin/BlueEngine/.be2-work/check-20260930T065711368418Z/report.json`. Final package smoke and actual-keyboard frames were inspected after engine integration.
