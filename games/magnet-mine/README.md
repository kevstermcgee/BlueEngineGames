# Magnet Mine

A polarity puzzle built on BlueEngine's custom-simulation kit. Move around the arena and press Space to alternate attraction and repulsion, guiding three inertial metal drones into glowing sockets.

## Controls

WASD moves, mouse looks, Space flips polarity, R restarts, F5/F9 saves and loads.

## Verification

```sh
cargo test --locked --no-default-features
python scripts/check.py --skip-ship
```

On Windows, double-click `Install-Desktop-Shortcut.cmd` to build the release,
package it, and place its launch shortcut on the Desktop.
