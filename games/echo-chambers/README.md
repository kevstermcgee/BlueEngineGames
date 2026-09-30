# Echo Chambers

Simon-says harmonic memory sequence across four resonance monoliths.

## Verification

Validate and simulate with `be2-tools`:
```sh
be2-tools game-validate game.json
be2-tools sim scenario.json
```

## Rules that need engine support

A wrong monolith now resets the sequence and re-arms the ones already struck (`wrong-*` rules come first: later rules see earlier changes). The strike rules are repeatable (`once` is per match and would block a retry). These use `fail`, `not_equals` and compound conditions or trigger zones (BlueEngine ADR 0023), so they need an engine built with that change.

## Scenarios

Each one is a headless run: `be2-tools sim FILE`.

- `scenario.json`: strike the monoliths north, south, east, west, then the altar: solved
- `scenario-out-of-order.json`: the altar does nothing before the sequence is complete
- `scenario-wrong-order.json`: strike east after north: the sequence resets, north is re-armed, and the full sequence still solves it
