# Wrecking Orbit

A momentum toy built on BlueEngine's custom-simulation kit. Two damped spring tethers follow the player's anchor; Space adds opposing tangential impulses so the wrecking balls can sweep through six crystal targets.

## Controls

WASD moves the anchor, mouse looks, Space whips the orbit, R restarts, F5/F9 saves and loads.

## Verification

```sh
cargo test --locked --no-default-features
python scripts/check.py --skip-ship
```

On Windows, double-click `Install-Desktop-Shortcut.cmd` to build the release,
package it, and place its launch shortcut on the Desktop.
