# Turret Trench

Cover-to-cover infiltration under cyclic turret sweeps using modulo timing arithmetic.

## Verification

Validate and simulate with `be2-tools`:
```sh
be2-tools game-validate game.json
be2-tools sim scenario.json
```

## Scenarios

```
be2-tools sim scenario.json               # full walk-through to completion
be2-tools sim scenario-out-of-order.json  # a disabled station is ignored
be2-tools sim scenario-late-win.json      # idle 300 ticks, then finish: the clock never blocks the win
```
