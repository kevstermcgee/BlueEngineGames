# Game screenshots

These PNGs are real captures from the games, used by both the catalog cards and
the game detail pages. Use `<game-slug>.png`, a landscape frame at least 320 × 180.
Prefer a clear gameplay view. Resize or letterbox the captured frame; do not
replace it with an icon, promotional illustration, or a generated image.

Before adding or replacing a screenshot, open the image and verify that it shows
the correct game. Square thumbnails and copies of `games/<slug>/assets/icon.png`
are rejected. CI checks every entry in `.release-games.json`; the site build also
checks every entry in the published release catalog. Missing screenshots fail
the build and preserve the previous deployment.

For games with the engine capture flags, run on an isolated virtual display:

```sh
python tools/xcapture.py /path/to/game --frames 30 --size 1280x720 --out /tmp/new-game-capture
```

This helper lives in the BlueEngine repository. Select a useful frame and resize
it to 640 × 360 (letterbox when needed), then save it here and run:

```sh
python3 -m unittest discover -s site -p 'test_*.py' -v
```

The October 2026 replacements for Lantern Grove, Lantern Run, Orchard Watch,
Pocket Breaker, Leo and Prop Hunt were captured from their native executables on
a virtual display. Pocket Breaker uses frame 3 to show the board during play.
Feta uses its `previews/feta/solo-feta.png` gameplay capture. Signal Garden uses
its gameplay verification capture; Slapstick uses a captured goal during a solo
match. The existing captures for the other games were visually reviewed.
