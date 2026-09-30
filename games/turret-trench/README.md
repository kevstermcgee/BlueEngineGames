# Turret Trench

Cover-to-cover infiltration under cyclic turret sweeps using modulo timing arithmetic.

## Verification

Validate and simulate with `be2-tools`:
```sh
be2-tools game-validate game.json
be2-tools sim scenario.json
```

## Rules that need engine support

The turret sweep is real: the override only works on odd `cycle_phase` (`modulo`), and pressing it on an even phase fails the match. These use the `fail` action and compound/modulo conditions (BlueEngine ADR 0023), so they need an engine built with that change.

## Scenarios

```
be2-tools sim scenario.json               # full walk-through to completion
be2-tools sim scenario-out-of-order.json  # a disabled station is ignored
be2-tools sim scenario-spotted.json  # hack the override while the turret faces you: the match is lost
```
