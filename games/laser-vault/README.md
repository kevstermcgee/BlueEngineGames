# Laser Vault

Cyber-heist security vault infiltration with laser tripwires and sequential hacking.

## Verification

Validate and simulate with `be2-tools`:
```sh
be2-tools game-validate game.json
be2-tools sim scenario.json
```

## Rules that need engine support

The tripwire is real: `laser_1` is a trigger zone across the straight path between the two consoles. Each entry adds an alarm; the third alarm ends the match with `fail`. Walk around it to keep the count at 0. This uses `fail` and `at_least` (BlueEngine ADR 0023), so it needs an engine built with that change.

## Scenarios

Each one is a headless run: `be2-tools sim FILE`.

- `scenario.json`: detour around the tripwire: hack a, hack b, loot the vault, no alarms
- `scenario-lockdown.json`: cross the tripwire three times: the third alarm locks the vault down (match lost)
- `scenario-out-of-order.json`: console b before console a does nothing (and the detour keeps the alarm at 0)
- `scenario-tripwire.json`: walk straight through the tripwire once: one alarm, still winnable
