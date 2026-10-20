"""Select validated native Cargo games for the two-platform review workflow."""
import argparse
import json
import os
from pathlib import Path
import re

from check_catalog import check, repository_file

DEFAULT_GAMES = (
    'deadfall', 'prop-hunt', 'slapstick', 'lantern-run', 'pocket-breaker',
    'orchard-watch', 'lantern-grove', 'signal-garden', 'idea-forge',
)
SLUG = re.compile(r'[a-z0-9]+(?:-[a-z0-9]+)*')
ENGINE_REPOSITORY = 'kevstermcgee/BlueEngine'


def source_file(root, relative):
    path = repository_file(root, relative)
    if any(part.is_symlink() for part in (path, *path.parents) if part.is_relative_to(root)):
        raise ValueError(f'Symlinked native check source is not allowed: {relative}')
    return path


def select(root, requested='all'):
    root = Path(root).resolve()
    if not isinstance(requested, str) or (requested != 'all' and (
            len(requested) > 80 or not SLUG.fullmatch(requested))):
        raise ValueError('Select all or one lowercase native game slug')
    # Validate the complete download metadata before choosing source to execute.
    check(root)
    catalog = json.loads((root / '.games-catalog.json').read_text())
    revision = catalog.get('source_revision')
    if (catalog.get('source_repository') != ENGINE_REPOSITORY
            or not isinstance(revision, str) or not re.fullmatch(r'[0-9a-f]{40}', revision)):
        raise ValueError('Native checks require the immutable upstream engine catalog pin')
    manifest = json.loads((root / '.release-games.json').read_text())
    definitions = {game['slug']: game for game in manifest['native_playables']}
    games = []
    for slug in DEFAULT_GAMES if requested == 'all' else (requested,):
        game = definitions.get(slug)
        if not game or game.get('kind') not in ('cargo', 'cargo-package'):
            raise ValueError(f'{slug}: select a registered cargo or cargo-package game')
        directory = f'games/{slug}'
        if game.get('directory') != directory:
            raise ValueError(f'{slug}: native checks require the matching games/<slug> source directory')
        source_file(root, directory + '/Cargo.toml')
        ship = game['kind'] == 'cargo-package'
        if ship:
            source_file(root, directory + '/scripts/ship.py')
        hub_test = root / directory / 'tests/engine_hub.rs'
        if hub_test.exists():
            source_file(root, directory + '/tests/engine_hub.rs')
        games.append(dict(slug=slug, directory=directory, ship=ship,
                          hub=slug == 'deadfall' or hub_test.is_file()))
    return dict(games=games, engine_revision=revision)


def write_outputs(destination, selection):
    # Values are validated identifiers or compact JSON; no input can add an
    # extra GITHUB_OUTPUT line, delimiter, command, matrix field or engine ref.
    with Path(destination).open('a', encoding='utf-8') as output:
        output.write('games=' + json.dumps(selection['games'], separators=(',', ':'), ensure_ascii=True) + '\n')
        output.write('engine_revision=' + selection['engine_revision'] + '\n')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--game-slug', default=os.environ.get('REQUESTED_GAME_SLUG', 'all'))
    parser.add_argument('--github-output', type=Path)
    args = parser.parse_args(argv)
    try:
        selection = select(args.root, args.game_slug)
        if args.github_output:
            write_outputs(args.github_output, selection)
    except (ValueError, KeyError, OSError) as error:
        parser.exit(1, f'Native game selection failed: {error}\n')
    print(json.dumps(selection, separators=(',', ':')))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
