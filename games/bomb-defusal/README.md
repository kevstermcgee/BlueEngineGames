# Bomb Defusal

High-stress timed crisis protocol requiring chronological wire-cutting across three stations.

## Verification

Validate and simulate with `be2-tools`:
```sh
be2-tools game-validate game.json
be2-tools sim scenario.json
```

## Rules that need engine support

The countdown is real: at 0 the `detonate` rule ends the match with `fail`. These use the `fail` action and compound/modulo conditions (BlueEngine ADR 0023), so they need an engine built with that change.

## Scenarios

```
be2-tools sim scenario.json               # full walk-through to completion
be2-tools sim scenario-out-of-order.json  # a disabled station is ignored
be2-tools sim scenario-timeout.json  # do nothing for 300 ticks: the fuse detonates and the match is lost
```
