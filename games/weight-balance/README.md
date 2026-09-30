# Weight & Balance

Mechanical equilibrium puzzle requiring ballast weight distribution across dual scale pans.

## Verification

Validate and simulate with `be2-tools`:
```sh
be2-tools game-validate game.json
be2-tools sim scenario.json
```

## Scenarios

Each one is a headless run: `be2-tools sim FILE`.

- `scenario.json`: place ballast on the left pan, then the right pan, then release the gate
- `scenario-right-first.json`: KNOWN GAP: the right pan is refused until the left is loaded, and there is no weighing: the puzzle is a fixed two-step order
