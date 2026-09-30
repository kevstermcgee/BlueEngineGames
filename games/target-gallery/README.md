# Target Gallery

Reaction marksmanship trial with mechanical pop-up silhouettes and dual visibility toggling.

## Verification

Validate and simulate with `be2-tools`:
```sh
be2-tools game-validate game.json
be2-tools sim scenario.json
```

## Scenarios

Each one is a headless run: `be2-tools sim FILE`.

- `scenario.json`: hit the left, centre and right targets, then claim the prize
- `scenario-double-press.json`: pressing a target repeatedly counts once (it is disabled and hidden after the first hit)
- `scenario-reverse-order.json`: hit the targets right to left: any order works
