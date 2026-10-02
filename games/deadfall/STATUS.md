# Deadfall status

Playable: solo with bots, hosting and joining (direct address), lobby with team choice, kill-target or timed matches, killcam,
results with Play again / Home, lifetime stats, controller support, 33 weapons, the Slagworks map, bots.

Known limits: matches cannot be joined once started (players wait for the next one); no player-versus-player body collision;
direct UDP needs a forwarded port or a VPN; controller input has not been tried with a physical pad.

## Movement, weapons and scenery maintenance (2026-10-01)

The local camera interpolates predicted movement ticks. Hand bob uses a continuous stride phase from predicted speed,
and look sway is damped using angular speed, so changing direction or speed no longer resets the animation.
The knife has articulated curled fingers, a tucked thumb and connected cuff, an angled ready pose, and a wind-up/contact/recovery motion whose
contact matches the simulation's 40% impact time. Heavy strikes use the same motion with more reach.

The killcam samples soldiers, dropped weapons, projectiles and zones from the camera's replay tick, replays recorded
combat effects, and preserves the victim until the death snapshot. It holds the aftermath until respawn instead of
following later fighting. The frame has smaller bars, replay progress and a labelled respawn countdown; live scores
and killfeed are hidden while replaying. Combat effects are timestamped at their received snapshot because the kit's
event-drain API does not expose the original event tick, so their timing remains approximate on delayed connections.

Trees render their round bark and branching meshes once, retaining the original solid trunk for collision. Raised soil
beds now show above the paving. Both scattered and hand-placed outdoor plants require a soil/gravel/grass footprint;
concrete, asphalt, roofs and props reject growth. The changed map heights also change the multiplayer fingerprint:
clients and servers should be rebuilt together.

Verification includes client and headless tests (combat, bot soaks, lossy networking and real UDP), movement/replay/plant
regressions, and Windows launch, icon, shortcut and smoke checks. Visual captures inspected include the knife grip at
0/90/270 degrees (`target/qa-knife-wrapped`), its first-person impact (`target/qa-knife-wrapped-contact`), trees and soil beds
(`target/qa-bed-shipped`), scripted walking/quiet walking/light/heavy strikes (`target/qa-play`), and a bot-driven killcam
(`target/qa-replay`). Scripted/autopilot captures continue without foreground focus on Windows; ordinary play still pauses.
