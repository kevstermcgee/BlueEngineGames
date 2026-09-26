# Riftwake

Riftwake is a networked **hyperkinetic arena FPS** built on BlueEngine. Its movement
rewards conserved momentum and air steering; its three-level arena rewards route
knowledge, item timing, prediction and weapon control. The default view is exactly
90 degrees, with no sprint key or aim-down-sights slowdown to interrupt the flow.

![Riftwake gameplay](preview.png)

## Play

```powershell
cargo run --release --locked -- --host
```

That starts a local authoritative server and joins it. Open another terminal and run
`cargo run --release --locked -- --connect 127.0.0.1:4200` for a second player.

For LAN/direct-IP play, the host runs:

```powershell
cargo run --release --locked --bin riftwake-server -- --listen 0.0.0.0:4200 --key YOUR_PRIVATE_KEY
```

Each player runs `cargo run --release --locked -- --connect HOST_IP:4200 --key YOUR_PRIVATE_KEY`.
UDP port 4200 must be allowed through the host firewall/router. The join key is for
trusted friend/LAN testing; it is not a public matchmaking or anti-cheat service.

## Controls

- WASD accelerates; hold Space to bunny hop immediately on landing
- Mouse looks and fires; 1-6 or wheel selects weapons
- Escape opens the game menu; F/F11 toggles fullscreen
- 90° FOV is the deliberate default and current competitive camera policy

## Arsenal

Ripper scattergun, Nailstorm, Grav Mortar, Rift Rocket, Arc Beam, and Void Rail cover
close burst, sustained tracking, arcing/splash pressure, prediction, and precision.
There are no reloads: the prototype keeps the focus on movement, selection and timing.

## The Fracture

The hand-authored duel arena has three elevations, six grounded stair routes, connected
upper bridges, a central mega-health power position, two armor circuits, two unobstructed
jump-pad shortcuts, framed gateways and broken rail sightlines. Structural piers ground
every deck. Concrete panels, blue steel walkways, service doors, windows and restrained safety markings establish an industrial facility.

## Prototype boundaries

The server owns movement, launch pads, hits, projectiles, splash, health/armor, pickups,
frag scoring and respawns for up to twelve direct-IP players. This slice does not yet
include bots, matchmaking, accounts, lag compensation, skeletal animation, content
downloads or public-server hardening. Those remain later milestones.

## Presentation update
F/F11 toggles fullscreen; Escape opens Resume / Controls / Quit. WASD and arrows move. Tab shows secondary stats and F3 shows diagnostics. Gameplay input is neutral while the menu is open; an online match continues. Host and server scripts use optimized builds.

Both the manifest and lockfile pin BlueEngine to commit `16acc82ddc3f934748ff5cf7e1f7a5ac74a289e3`. Cargo fetches that revision directly; a sibling engine checkout is not required.

## Readability and movement revision
Shared immutable font atlas, beveled 3D weapon models, muzzle flash/recoil, shot audio,
tracers and wall impacts replace the old text and flat weapon blocks. Shot feedback is
cosmetic; damage remains server-owned. Local camera smoothing sweeps the collision
hull so presentation cannot extrapolate through map edges. This remains a low-poly
prototype, not a photorealistic asset pack. The client waits for the authoritative
spawn snapshot before sending look input, preserving the intended starting direction.
