# Pulse Nova

High-velocity kinetic arena shooter built on BlueEngine.

Blast through waves of cybernetic drones, chain multi-kill explosions, leap across high-altitude jump pads, and survive the surge in a floating neon colosseum.

## Controls

- **WASD**: Move
- **Mouse**: Aim
- **Left Click / E**: Pulse Blaster (kinetic plasma bolts)
- **Right Click / Shift**: Kinetic Dash (burst thrust & invulnerability)
- **Space**: Jump
- **Jump Pads**: Launch into high-altitude aerial combat
- **F5 / F9**: Quick-Save and Quick-Load
- **R**: Restart run
- **Esc**: System Menu

## Verification

```sh
cargo test --locked
python scripts/check.py --skip-ship
```

On Windows, double-click `Install-Desktop-Shortcut.cmd` to build the release package and place the launch shortcut on your Desktop.
