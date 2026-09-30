# Phantom Maze

Blind labyrinth navigation with active sonar ping sensor pads to reveal hidden walls.

## Verification

Validate and simulate with `be2-tools`:
```sh
be2-tools game-validate game.json
be2-tools sim scenario.json
```

## Scenarios

Each one is a headless run: `be2-tools sim FILE`.

- `scenario.json`: ping both sonar beacons, then take the exit
- `scenario-premature-exit.json`: KNOWN GAP: pinging beacon 2 first arms the exit early; with one ping the exit does nothing
