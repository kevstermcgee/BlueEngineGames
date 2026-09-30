# Physics Lab

A standalone first-person BlueEngine experiment room. Launch samples with E and cycle materials with M. Six material presets compare density, restitution, friction and flammability. Walk between the drop-test lane, water-fed fire tray, buoyancy tank and live planar mirror.

Controls: WASD/mouse, Space jump; E launch; M material; 1 water; 2 fire; L daylight/sunset/night/rotating pulse; G Earth/moon gravity; B wind; R reset; F5/F9 engine quick save/load; Esc menu. A maximum of 90 spheres and 360 droplets bounds cost.

The simulation is a deterministic 60 Hz sphere contact model with density-weighted collision impulses. The tank uses an approximate immersed fraction and Archimedes acceleration with drag. Droplets follow gravity, cool hot samples and disappear at ground contact. Flammable samples consume fuel above the ignition temperature. This is a test bench, not a calibrated scientific solver: no SPH/Navier–Stokes, angular rigid bodies, fracture, glass refraction or ice melting. The engine’s PlanarMirror rerenders the scene through the reflected eye and an off-axis aperture; it shows a player proxy. Lighting has four patterns plus localized flickering fire and burning-sample lights. The rotating mode adds a moving blue point light. It does not cast dynamic shadows.

With BlueEngine checked out alongside BlueEngineGames (commit `17d90e9` or later), run `scripts/blue ship` to build/package and install the Physics Lab desktop shortcut. Run `python3 scripts/check.py` to validate. Headless evidence: `python3 ../../../BlueEngine/tools/xcapture.py dist/physics-lab --frames 30,120 --out captures -- --script 'drop@40,material@50,light@70'`.

Rules live in `src/lib.rs`, rendering/input in `src/main.rs`. Save state includes every simulation field. Tests cover input toggles, bounded population, fire/quenching, density-dependent floating, determinism and exact save continuation.
