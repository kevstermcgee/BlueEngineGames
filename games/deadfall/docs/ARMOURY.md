# Deadfall armoury

Generated from `src/weapons.rs` (`cargo test --lib write_armoury_doc -- --ignored`). Damage is per bullet or
pellet to the torso at point blank (players have 100 health); `TTK` is seconds of shooting for a 100-health
body kill at 10 m with every pellet landing; `shots` is trigger pulls for that kill. Speed is the fraction
of unarmed run speed. Rates of fire are the game's cyclic rate (capped below the real one where noted in
the source so no gun kills in under about a third of a second).

| id | key | name | real counterpart | class | fire | dmg | head | shots | TTK | rpm | mag/res | reload | range | speed | sight |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | k9 | K-9 Sidearm | Glock 17, 9x19mm | Pistol | semi | 24 | x2 | 5 | 0.73 | 330 | 17/51 | 1.7s | 25 m | 0.98 | iron |
| 2 | m45 | Bulldog .45 | Colt M1911A1, .45 ACP | Pistol | semi | 31 | x2 | 4 | 0.67 | 270 | 7/28 | 1.9s | 25 m | 0.97 | iron |
| 3 | hc50 | Hand Cannon | Desert Eagle, .50 AE | Pistol | semi | 55 | x2 | 2 | 0.63 | 95 | 7/28 | 2.2s | 35 m | 0.96 | iron |
| 4 | rv357 | Marshal .357 | Colt Python revolver, .357 Magnum | Pistol | semi | 40 | x2 | 3 | 0.92 | 130 | 6/24 | 3.3s | 40 m | 0.96 | iron |
| 5 | mp9 | MP-9 | Heckler & Koch MP5, 9x19mm | Smg | auto | 19 | x2 | 6 | 0.38 | 800 | 30/120 | 2.1s | 30 m | 0.95 | iron |
| 6 | ump | UMP-45 | H&K UMP45, .45 ACP | Smg | auto | 24 | x2 | 5 | 0.40 | 600 | 25/100 | 2.3s | 32 m | 0.94 | dot |
| 7 | pdw | PDW-57 | FN P90, 5.7x28mm | Smg | auto | 18 | x2 | 6 | 0.38 | 800 | 50/150 | 2.7s | 40 m | 0.95 | dot |
| 8 | vkr | Vektor-K | KRISS Vector, .45 ACP | Smg | auto | 18 | x2 | 6 | 0.36 | 840 | 25/100 | 2.0s | 28 m | 0.94 | dot |
| 9 | k47 | K-47 | AK-47, 7.62x39mm | AssaultRifle | auto | 33 | x2 | 4 | 0.40 | 450 | 30/120 | 2.5s | 55 m | 0.90 | iron |
| 10 | m4c | M4-C Carbine | Colt M4A1, 5.56x45mm | AssaultRifle | auto | 26 | x2 | 4 | 0.36 | 500 | 30/120 | 2.0s | 60 m | 0.91 | dot |
| 11 | fm2 | F-2 Burst | FAMAS F1, 5.56x45mm (3-round burst) | AssaultRifle | burst x3 | 29 | x2 | 4 | 0.36 | 250 | 25/100 | 2.4s | 60 m | 0.90 | iron |
| 12 | bpa | Bullpup A1 | Steyr AUG, 5.56x45mm (1.5x optic) | AssaultRifle | auto | 24 | x2 | 5 | 0.37 | 650 | 30/120 | 2.3s | 65 m | 0.90 | 1.5x |
| 13 | gl4 | G-4 Battle Rifle | H&K G3, 7.62x51mm | Dmr | auto | 40 | x2 | 3 | 0.36 | 330 | 20/80 | 2.7s | 70 m | 0.86 | iron |
| 14 | dmr20 | DMR-20 | SCAR-20 / SR-25, 7.62x51mm (3x scope) | Dmr | semi | 44 | x2.4 | 3 | 0.50 | 240 | 20/60 | 2.5s | 90 m | 0.87 | 3x |
| 15 | svd | SVD Marksman | Dragunov SVD, 7.62x54R (4x scope) | Dmr | semi | 50 | x2.2 | 2 | 0.40 | 150 | 10/30 | 2.9s | 95 m | 0.86 | 4x |
| 16 | scout | Scout | Steyr Scout / SSG 08 bolt, .308 (4x scope) | Sniper | bolt/pump 1.20s | 85 | x2 | 2 | 1.20 | - | 10/30 | 3.3s | 110 m | 0.90 | 4x |
| 17 | awm | AW-M Magnum | Accuracy International AWM, .338 Lapua (6x scope) | Sniper | bolt/pump 1.70s | 110 | x2 | 1 | 0.00 | - | 5/20 | 3.4s | 140 m | 0.80 | 6x |
| 18 | m82 | Anvil .50 | Barrett M82, .50 BMG semi-auto (8x scope) | Sniper | semi | 130 | x2 | 1 | 0.00 | 90 | 10/20 | 3.8s | 150 m | 0.72 | 8x |
| 19 | pump12 | Pump-12 | Remington 870, 12 gauge, pump | Shotgun | bolt/pump 0.95s | 12 x9 | x1.5 | 1 | 0.00 | - | 8/32 | 0.6s/shell | 16 m | 0.88 | iron |
| 20 | auto12 | Auto-12 | Benelli M4 / XM1014, 12 gauge semi-auto | Shotgun | semi | 10 x8 | x1.5 | 2 | 0.25 | 240 | 7/28 | 0.5s/shell | 15 m | 0.86 | iron |
| 21 | sawn | Sawn-Off | double-barrel 12 gauge, 2 shells | Shotgun | semi | 12 x9 | x1.5 | 1 | 0.00 | 400 | 2/24 | 2.3s | 9 m | 0.92 | iron |
| 22 | para | Para-SAW | FN Minimi / M249, 5.56x45mm belt | Lmg | auto | 28 | x2 | 4 | 0.41 | 440 | 100/100 | 5.4s | 55 m | 0.78 | iron |
| 23 | pk | PK-74 | PKM, 7.62x54R belt | Lmg | auto | 32 | x2 | 4 | 0.45 | 400 | 100/100 | 6.2s | 65 m | 0.72 | iron |
| 24 | rpg | Kobra RL | RPG-7 rocket launcher | Launcher | throw | 130 | x1 | 1 | 0.00 | 12 | 1/2 | 4.3s | 120 m | 0.75 | iron |
| 25 | thumper | Thumper GL | M79 40mm grenade launcher | Launcher | throw | 90 | x1 | 2 | 3.00 | 20 | 1/6 | 2.4s | 100 m | 0.85 | iron |
| 26 | frag | Frag Grenade | M67 fragmentation grenade | Grenade | throw | 100 | x1 | 1 | 0.00 | 40 | 1/0 | - | 30 m | 1.00 | - |
| 27 | flash | Flashbang | M84 stun grenade | Grenade | throw | 0 | x1 | - | - | 40 | 1/0 | - | 30 m | 1.00 | - |
| 28 | smoke | Smoke Grenade | M18 smoke grenade | Grenade | throw | 0 | x1 | - | - | 40 | 1/0 | - | 30 m | 1.00 | - |
| 29 | incen | Incendiary | M14 incendiary grenade | Grenade | throw | 20 | x1 | - | - | 40 | 1/0 | - | 30 m | 1.00 | - |
| 30 | knife | Combat Knife | USMC Ka-Bar | Melee | swing | 35/65 | x1 | 3 | 0.80 | - | - | - | 1.6 m | 1.00 | - |
| 31 | machete | Machete | jungle machete | Melee | swing | 42/78 | x1 | 3 | 1.10 | - | - | - | 1.9 m | 0.98 | - |
| 32 | axe | Breaching Axe | fire axe | Melee | swing | 55/100 | x1 | 2 | 0.80 | - | - | - | 2 m | 0.90 | - |
| 33 | crowbar | Crowbar | steel crowbar | Melee | swing | 30/60 | x1 | 4 | 1.50 | - | - | - | 1.8 m | 0.96 | - |

## New pickups

The original weapon IDs 1–33 are unchanged.

| ID | Key | Weapon | Role |
|---|---|---|---|
| 34 | hornet | Hornet Burst | Three-shot secondary, 18-round magazine, 22 damage per shot; close-range bursts. |
| 35 | ranger | Ranger Lever Rifle | Eight-round primary, 78 damage, 0.6-second cycle, 2× scope; rewards accurate shots. |
| 36 | breach8 | Breach-8 Slug | Eight-round primary, one 96-damage slug per shot; precise short-to-medium-range pressure. |
