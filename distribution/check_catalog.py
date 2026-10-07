"""Check download coverage before compiling or publishing any game."""
import argparse
import json
from pathlib import Path
import re


def repository_path(root, value):
    if not isinstance(value, str) or not value or "\\" in value or ":" in value:
        raise ValueError(f"Unsafe repository path: {value!r}")
    path = root / value
    if value.startswith("/") or any(p in ("", ".", "..") for p in value.split("/")):
        raise ValueError(f"Unsafe repository path: {value!r}")
    if not path.resolve().is_relative_to(root.resolve()) or not path.exists():
        raise ValueError(f"Missing or escaping repository path: {value}")
    return path


def repository_file(root, value):
    path = repository_path(root, value)
    if not path.is_file():
        raise ValueError(f'Expected a file, found a directory: {value}')
    return path


def check(root):
    root = Path(root)
    catalog = json.loads((root / '.games-catalog.json').read_text())
    manifest = json.loads((root / '.release-games.json').read_text())
    if manifest.get('version') != 1:
        raise ValueError('Unsupported release manifest version')
    definitions = catalog['playables'] + manifest['data_playables'] + manifest['native_playables']
    slugs, covered = set(), set()
    for game in definitions:
        slug = game['slug']
        if not re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*', slug) or slug in slugs:
            raise ValueError(f'Unsafe or duplicate download slug: {slug}')
        slugs.add(slug)
        if 'directory' in game:
            directory = repository_path(root, game['directory'])
            covered.add(game['directory'])
            kind = game['kind']
            if kind in ('cargo', 'cargo-package'):
                repository_file(root, game['directory'] + '/Cargo.toml')
                if not game.get('binary'):
                    raise ValueError(f'{slug} has no playable binary')
                if kind == 'cargo-package':
                    repository_file(root, game['directory'] + '/scripts/ship.py')
            elif kind == 'release-asset':
                source = json.loads((directory / 'release-source.json').read_text())
                if not re.fullmatch(r'https://github\.com/[\w.-]+/[\w.-]+/releases/download/[^/]+/[^/]+', source['url']):
                    raise ValueError(f'{slug} needs a pinned GitHub release URL')
                if not re.fullmatch(r'[0-9a-f]{64}', source['sha256']):
                    raise ValueError(f'{slug} needs a SHA256-pinned release asset')
                if not source.get('files') or source['executable'] not in source['files']:
                    raise ValueError(f'{slug} needs a declared executable and payload')
            elif kind != 'engine-sandbox':
                raise ValueError(f'Unknown native release kind: {kind}')
        else:
            if not game.get('arguments') or not game.get('files'):
                raise ValueError(f'{slug} needs data files and launch arguments')
            for name in game['files']:
                repository_path(root, name)
                covered.add('/'.join(name.split('/')[:2]))
    for name in manifest['non_playable_directories']:
        repository_path(root, name)
        if name in covered:
            raise ValueError(f'Playable also marked non-playable: {name}')
        covered.add(name)
    for name in manifest['game_roots']:
        directory = repository_path(root, name)
        for game in directory.iterdir():
            if game.is_dir() and f'{name}/{game.name}' not in covered:
                raise ValueError(f'Game has no Windows download definition: {name}/{game.name}')
    return slugs


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args()
    try:
        slugs = check(args.root)
    except (ValueError, KeyError, OSError) as error:
        parser.exit(1, f'Catalog coverage failed: {error}\n')
    print(f'Validated {len(slugs)} Windows download definitions.')
