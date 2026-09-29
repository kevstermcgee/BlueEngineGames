# Skyhook Sprint

A matched BlueEngine/RedEngine physics test. Chain three fixed-step launch pads through ordered airborne gates. Missing a landing drops the player into the void.

## Controls

WASD moves, mouse looks, Space jumps, R restarts, F5/F9 saves and loads.

## Verification

```sh
cargo test --locked --no-default-features
python scripts/check.py --skip-ship
```

On Windows, double-click `Install-Desktop-Shortcut.cmd` to build the release,
package it, and place its launch shortcut on the Desktop.
