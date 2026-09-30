# Prism Matrix

Optical laser beam redirection matrix requiring multi-station angular alignment.

## Verification

Validate and simulate with `be2-tools`:
```sh
be2-tools game-validate game.json
be2-tools sim scenario.json
```

## Scenarios

Each one is a headless run: `be2-tools sim FILE`.

- `scenario.json`: align prisms 1, 2, 3 then energize the mainframe
- `scenario-premature-terminal.json`: KNOWN GAP: aligning prism 3 first arms the mainframe early; pressing it with one prism aligned does nothing
- `scenario-reverse-order.json`: align prisms 3, 1, 2: order does not matter
