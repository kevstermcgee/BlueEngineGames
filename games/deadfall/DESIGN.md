# Deadfall: design brief (read this first)

Deadfall is a team-deathmatch shooter for up to 12 players (two teams of six), built on the BlueEngine
(`vesper3d` = the engine crate `be2`, path dependency). It ships as one Windows `.exe`: **every model, texture-less
surface, sound and map is generated in code**, there are no asset files. The look is clean low-poly: flat-coloured
boxes, cylinders, cones and ellipsoids lit by the engine's `kit` materials (hemispheric ambient, one key light, rim
light, fog, up to four point lights). There are no textures and no shadows.

## Hard rules from the brief
* First-person by default, 90 degree vertical FOV (aiming narrows it), iron sights and scopes, crouching.
* **No blood. No music. No radio chatter.** Natural sounds of the place, weapons, explosions only.
* HUD extremely minimal.
* Teams: **Ironclad** (army green uniform, tan vest, boots and gloves, olive helmet with a tan net) and
  **Nightwatch** (navy uniform, black vest, black helmet, black gloves and boots). Both wear military helmets.
  Hands and arms in first person must match the wearer: same sleeve colour, same gloves.
* At least 25 distinct weapons; 33 are planned (the roster below). Two firearms (one primary, one secondary), one
  melee weapon, up to two grenades per player. Every weapon has its own ammunition (magazine and reserve).
* Units: metres. +Y up. Yaw 0 faces -Z, positive yaw turns towards +X. A standing player is 1.80 m tall, eye at
  1.68 m; crouched 1.10 m (eye at about 0.98 m); radius 0.23 m.

## Roster (wire id = position in this list, starting at 1; keys are final)
| id | key | name | real counterpart |
|---|---|---|---|
| 1 | k9 | K-9 Sidearm | Glock 17, 9x19mm |
| 2 | m45 | Bulldog .45 | Colt M1911A1, .45 ACP |
| 3 | hc50 | Hand Cannon | Desert Eagle, .50 AE |
| 4 | rv357 | Marshal .357 | Colt Python revolver, .357 Magnum |
| 5 | mp9 | MP-9 | Heckler & Koch MP5, 9x19mm |
| 6 | ump | UMP-45 | H&K UMP45, .45 ACP |
| 7 | pdw | PDW-57 | FN P90, 5.7x28mm |
| 8 | vkr | Vektor-K | KRISS Vector, .45 ACP |
| 9 | k47 | K-47 | AK-47, 7.62x39mm |
| 10 | m4c | M4-C Carbine | Colt M4A1, 5.56x45mm |
| 11 | fm2 | F-2 Burst | FAMAS F1, 5.56x45mm (3-round burst) |
| 12 | bpa | Bullpup A1 | Steyr AUG, 5.56x45mm (1.5x optic) |
| 13 | gl4 | G-4 Battle Rifle | H&K G3, 7.62x51mm |
| 14 | dmr20 | DMR-20 | SCAR-20 / SR-25, 7.62x51mm (3x scope) |
| 15 | svd | SVD Marksman | Dragunov SVD, 7.62x54R (4x scope) |
| 16 | scout | Scout | Steyr Scout / SSG 08 bolt, .308 (4x scope) |
| 17 | awm | AW-M Magnum | Accuracy International AWM, .338 Lapua (6x scope) |
| 18 | m82 | Anvil .50 | Barrett M82, .50 BMG semi-auto (8x scope) |
| 19 | pump12 | Pump-12 | Remington 870, 12 gauge, pump |
| 20 | auto12 | Auto-12 | Benelli M4 / XM1014, 12 gauge semi-auto |
| 21 | sawn | Sawn-Off | double-barrel 12 gauge, 2 shells |
| 22 | para | Para-SAW | FN Minimi / M249, 5.56x45mm belt |
| 23 | pk | PK-74 | PKM, 7.62x54R belt |
| 24 | rpg | Kobra RL | RPG-7 rocket launcher |
| 25 | thumper | Thumper GL | M79 40mm grenade launcher |
| 26 | frag | Frag Grenade | M67 fragmentation grenade |
| 27 | flash | Flashbang | M84 stun grenade |
| 28 | smoke | Smoke Grenade | M18 smoke grenade |
| 29 | incen | Incendiary | M14 incendiary grenade |
| 30 | knife | Combat Knife | USMC Ka-Bar |
| 31 | machete | Machete | jungle machete |
| 32 | axe | Breaching Axe | fire axe |
| 33 | crowbar | Crowbar | steel crowbar |

Starting weapons (both teams identical): `k9` and `knife`. Everything else is found on the map.

## The map: "Slagworks"
An industrial complex (foundry / steelworks) about 120 m by 90 m, two bases at opposite ends, three routes
between them, with different buildings: a smelter hall with furnace pits and catwalks, a two-floor control office,
a warehouse, a container yard, a pipe alley, a loading dock, a boiler house, a rail yard with cars, a water tower and
cooling towers as landmarks. Overgrown in places: trees, bushes, weeds breaking through concrete, potted plants in
the office. Overcast afternoon.

## Code layout
`src/lib.rs` is the pure game library (no window): `weapons` (the armoury table), `level` (map data),
`sim` (authoritative match), `bots`, `nav`, `netgame` (network layouts), `stats`. `src/client/` is the window side
(needs the `client` feature): renderer, models, menus, HUD, audio. Binaries: `deadfall` (the game), `deadfall-server`
(dedicated server), `preview` and friends (look at one thing at a time).

## Looking at your work (you cannot watch a window)
`src/client/previewkit.rs` is a stage: an orbit camera, a floor, PNG capture. Write a small binary in `src/bin/`
(see `preview.rs`) that draws what you built, then:
```
cargo build --bin <name>
xvfb-run -a -s "-screen 0 1280x720x24" ../../../../target-dir/debug/<name> --out DIR --angles 0,90,180,270 --dist 2 --at 0,1,0
```
then open the PNGs with the Read tool and LOOK. Software rendering is slow (about 16 fps): ask for few frames.
Judge proportions, silhouette, colour, whether parts are attached to each other, and the real-world scale.
Do not claim something looks right until you have looked at it from at least three angles.

## Engine API you will use (`vesper3d::viewer::kit`)
`Template::new()` then `box_(center, half, rgb, glow)`, `box_top`, `ball(center, radii, rgb, glow, segs, rings)`,
`cone(base, r_bottom, r_top, height, rgb, glow, sides)`, `cylinder(base, r, height, rgb, glow, sides)` (upright,
Y axis), `ring`, `disc`, `tube`, `quad_facing`, `transformed(Mat4)` (rotate/scale/position a whole template: use it to
lay a cylinder on its side), `append(&other)`. `rgb` is `[f32;3]` in 0..1, `glow` 0..1 self-illumination. Boxes are
axis-aligned (use `transformed` to rotate). Keep templates under 9000 vertices each (they are split for you above that).
`Batch::add(&template, Mat4, Tint)` then draw. Read `~/BlueEngine-fc/docs/CUSTOM_SIM_CHEATSHEET.md` for the rest. There
is no texture support: detail comes from many small, well-coloured parts. Use subtle colour variation between parts
(dark metal, lighter metal, polymer black, wood brown) rather than one flat colour.

## Working rules
* Work only in your own files (listed in your task). Do not edit files you were not given; if you need a change in
  a shared file, say so in your final report instead.
* `cargo fmt`, no warnings, no `unsafe`, no new dependencies. Unit tests for anything that is logic.
* Use `CARGO_TARGET_DIR=/home/kevin/deadfall-target` (shared build cache). Build with `cargo build --bin X` and
  `cargo test --lib`.
* Commit your work on your branch with clear messages. Do not push.
* Final report: what you built, what you verified and how (name the screenshots you looked at), what is missing.
