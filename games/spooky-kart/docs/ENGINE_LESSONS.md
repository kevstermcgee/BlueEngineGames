# What Spooky Kart taught us about BlueEngine

Candidate engine improvements, each with the evidence that suggested it. Nothing here has been changed in the
engine yet; each is a self-contained piece of work. Numbers come from `docs/perf` in BlueEngine
(`kart_*` metrics) and from the tests in this repository.

## Networking

1. **A generic custom-simulation netcode kit.** The engine's `DedicatedServer` and `Packet` formats are built
   around walking characters, so a game with its own simulation had to write its own server and client. About
   1,000 lines of that are not kart-specific: the lobby/race/results loop, session handling on
   `SessionRegistry`, per-player input buffering, snapshot pacing, event delivery, bot takeover on
   disconnect, per-race statistics (`src/server.rs`), and the client's connect handshake, prediction,
   reconciliation and interpolation (`src/client.rs`). Extract them behind a `Simulation`-style trait so the next
   custom-sim multiplayer game needs only its rules and its snapshot layout.
2. **A binary codec.** The engine has none (JSON over UDP). `src/wire.rs` is a bounds-checked writer/reader with
   round-trip, truncation and random-garbage tests; a snapshot of eight karts is 584 bytes typically and 1,112 at
   worst, against a 1,200-byte budget, about 18 KB/s per client at 30 Hz. Promote the reader/writer and its test
   pattern into `viewer::net`.
3. **Redundant input bundles.** Sending the last four inputs in every datagram meant 10% packet loss cost the
   server almost nothing (25 repeated ticks out of 27,574 inputs in `a_laggy_lossy_network_still_produces_a_finished_race`).
   Worth making the default input strategy.
4. **Reliable lobby intent.** A one-shot "Ready" was lost under 10% loss and stalled the lobby. Choices that must
   arrive (ready, character) are now re-sent until the server's state matches (`KartClient::sync_lobby`). The
   engine's `reliable_command` could offer this as a small "desired state" primitive.
5. **Prediction of collisions.** The client predicts its own kart alone, so bumps with other karts show up as
   corrections: 2 corrections in a 1-client race but 1,328 in an 8-client one (largest error still under 0.9 m,
   no snaps). Predicting against the interpolated rivals would remove most of them.
6. **An in-memory network with delay, jitter and loss.** `transport::LoopNet` lets a full server and eight
   clients race in under a second of wall time, deterministically. The engine's `NetworkSimulator` only handles
   its own `Packet` type; a raw-datagram version (this one) is the more useful shape.
7. **The 8-player cap.** `DedicatedServer` hard-codes 8 sessions (`server.rs:230`); Spooky Kart's server takes
   the limit from `SessionRegistry::new(max, timeout)`. Make the engine's configurable too.

## Rendering

8. **A template over 9,000 vertices vanishes silently.** `Batch::add` skips it without a message, so the whole
   Haunted Hollow rendered as an empty purple void until `models::Chunks` split it. `Template::to_meshes` should
   split automatically, or `Batch::add` should say so loudly.
9. **Quad winding.** `Template::quad` wants counter-clockwise corners seen from the front; the wrong order is
   back-face culled with no hint. A `quad_facing(normal)` helper (`models::face` here) would remove the trap.
10. **Capturing on a headless box.** `xvfb-run` with software GL works but runs at roughly 16 frames per second,
    so a whole race takes about 7 minutes to capture. The recipe is in STATUS.md; a documented offscreen mode
    would be quicker.

## Process

11. **Bots as clients are the best test tool we found.** The bot logic reads the same `Sim` a client holds, so it
    drives a real client (`tests/net.rs`), fills the grid, and generates load (`tools/load_test.py`) with no
    extra code. Games that ship with AI drivers get load testing for free.
12. **Per-race telemetry from day one.** `RaceReport` plus network statistics in `matches.jsonl` made balance
    ("Frankenstein's Monster won 11 of 24 bot races before tuning") and network quality measurable, not felt.
