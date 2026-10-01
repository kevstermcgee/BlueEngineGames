# Deadfall: what the build cost

Measured during development, 2026-09-30 23:21 UTC to 2026-10-01 01:15 UTC (about 1 hour 55 minutes of wall-clock time).
Token counts are the model's own accounting (the conversation's remaining-token counter for the lead agent; each helper agent's
reported usage). They are exact where stated and rounded where marked "about".

## Totals
| | Tokens | Share |
|---|---|---|
| Lead agent (planning, engine reading, game core, client app, tests, integration, CI) | about 560,000 | 23% |
| Helper agents, first wave (5, in parallel) | 1,144,125 | 47% |
| Helper agents, redesign wave after your feedback (3) | 739,352 | 30% |
| **Everything** | **about 2,440,000** | |

## Helper agents (each worked alone in its own git worktree)
| Job | Tokens | Tool calls | Time |
|---|---|---|---|
| Armoury table (33 weapons, tests) | 92,705 | 5 | 6 min |
| Procedural audio (105 cues, stereo pan variants, 4 ambience loops) | 160,905 | 36 | 16 min |
| Soldiers and first-person arms, version 1 | 201,120 | 66 | 17 min |
| Weapon models, version 1 | 329,680 | 110 | 29 min |
| Slagworks map and its renderer | 359,715 | 104 | 36 min |
| First-person hands, redo (later replaced by a simple version written by the lead) | 221,183 | 87 | 18 min |
| Soldier bodies, redo | 164,754 | 47 | 13 min |
| Weapon models, redo (rounded, curved shapes) | 353,415 | 126 | 31 min |

## What cost the most
1. **Art.** Weapons (683,000 tokens in two passes), the map (360,000), soldiers (366,000) and hands (221,000 plus several rewrites by
   the lead) were 1.63 of the 2.44 million tokens, 67% of everything. Procedurally modelling without textures or imported meshes
   means every look decision is code plus a screenshot loop, and the first versions were too blocky, too thin and had awkward hands:
   **the three "redo" passes you asked for cost 739,000 tokens (30%) and about an hour.** Cheapest lesson: a shape toolkit (lofts, sweeps,
   rounded boxes) should exist before the first model, not after the first complaint.
2. **Reading the engine** (about 110,000 lead tokens): the netplay kit, the `kit` renderer, `Controller`, audio and the release scripts. No
   single document says how a networked shooter fits together; I read source.
3. **Game core and client app** (about 230,000 lead tokens): `sim`, `hands`, `netgame`, `bots`, `nav`, and the 1,500-line app.
4. **Waiting.** A Windows build of the release workflow takes 20-25 minutes on CI and there is no local cross-compiler, so the
   Windows build could be checked only twice (once mid-way, once at the end). Local debug builds took 1-3 minutes; a release
   build 8 minutes the first time.

## Defects found by tests or screenshots (not by luck) and what they cost
* Bot grenades were never thrown: the bot switched weapons on the same tick it released the pin, cancelling the throw (found by a test).
* Nav stairs: a 0.5 m grid is coarser than 0.3 m stair treads, so bots could not climb stairs (unit test).
* Aiming down sights filled the screen with the rear sight until an eye-relief offset was added; the scope mask was a polygon until redone.
* Glass drawn with the engine's blended material showed through walls from across the map (no depth test): drawn opaque instead.
* The engine's native keyboard reader tracks only 23 keys: reload, drop and the scoreboard would have been dead keys on Windows.
* A `Tint`-less muzzle flash drew as a white square: replaced with crossed beams.
* Three of the first nine combat tests were wrong about timing (lag compensation off-by-one, walk direction): fixed by deriving the aim from the server's own history.

## Tests as they stand
103 library tests; integration tests: combat (8), 12-bot soaks on 4 seeds plus a grenade-throw check (3), pickups and inventory limits on the real map (3),
online match on a lossy simulated network (3), 20 seconds of real UDP with 6 players and 6 bots (1). The UDP test measured 90 kbit/s per
client downstream, 28 snapshots per second, the slowest server tick 5.8 ms.

## Not verified (said plainly)
* The Windows executable was built by CI (two successful runs of the full release workflow on this branch) but **never run by me on Windows**.
* **No physical controller** and **no real keyboard/mouse events** were available: input was verified through scripted input and
  unit tests only (there is no X input-injection tool on this machine).
* **Nothing was heard**: sound is verified by numbers (levels, spectra, loop seams, pan balance), not by ear.
* Not played by a human against other humans; online play was exercised by simulated and scripted clients over loopback.
