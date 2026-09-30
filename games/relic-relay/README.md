# Relic Relay

Fragile courier dash with decay timers and mid-point stabilization recharge.

## Verification

Validate and simulate with `be2-tools`:
```sh
be2-tools game-validate game.json
be2-tools sim scenario.json
```

## Rules that need engine support

The decay is real: while the relic is carried and unstabilized, `relic_charge` drops each timer pulse and the relic shatters (`fail`) at 0. Reaching the recharge pylon stops the decay. These use `fail`, `not_equals` and compound conditions or trigger zones (BlueEngine ADR 0023), so they need an engine built with that change.

## Scenarios

Each one is a headless run: `be2-tools sim FILE`.

- `scenario.json`: pick up the relic, stabilize it at the pylon in time, deliver it
- `scenario-decay.json`: carry the relic too long without stabilizing it: it shatters (match lost) and the pylon no longer helps
- `scenario-out-of-order.json`: the pylon does nothing before the relic is picked up
