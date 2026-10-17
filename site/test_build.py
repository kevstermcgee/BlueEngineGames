import csv
import copy
import importlib.util
import json
import struct
import subprocess
import tempfile
import unittest
import zlib
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('download_site', Path(__file__).with_name('build.py'))
site = importlib.util.module_from_spec(spec)
spec.loader.exec_module(site)


def release(tag, names, **extra):
    return {'tag_name': tag, 'published_at': '2026-10-01T00:00:00Z',
            'assets': [{'name': name, 'size': 1234,
                        'browser_download_url': f'https://example.com/{tag}/{name}'} for name in names], **extra}


def screenshot(width=320, height=180, color=b'\x20\x60\x80'):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress((b'\0' + color * width) * height)) + chunk(b'IEND', b''))


def wiki_fixture(root, slug, origin='human'):
    game = Path(root) / 'games' / slug
    (game / 'assets/wiki').mkdir(parents=True, exist_ok=True)
    gallery = []
    for index in range(3):
        name = f'assets/wiki/gameplay-{index}.png'
        (game / name).write_bytes(screenshot(color=bytes([32 + index, 96, 128])))
        gallery.append({'image': name, 'caption': f'Gameplay challenge {index + 1}.', 'alt': f'The player taking on challenge {index + 1}.'})
    document = {'version': 1,
                'overview': 'Explore a sequence of spatial puzzles by moving blocks between marked goals. Each stage introduces a new constraint, and the player can retry any move to find a better solution.',
                'rules': ['Move blocks onto all of the marked goal squares.', 'A block moves only when the square beyond it is empty.', 'Complete every goal to advance to the following stage.'],
                'controls': ['Use arrow keys to move and R to retry.'],
                'story': 'This is an abstract puzzle collection without a narrative campaign or named characters.',
                'characters': [], 'characters_note': 'The player manipulates blocks; there are no narrative characters.',
                'gallery': gallery,
                'design': {'origin': origin, 'original_prompt': 'Create a spatial puzzle game about moving blocks onto goal squares.' if origin == 'human' else None,
                           'rationale': 'The design explores how a simple pushing rule can create increasingly interesting spatial decisions. Its focused puzzle format supports reversible experimentation and a varied sequence of challenges.' if origin == 'ai' else None}}
    (game / 'wiki.json').write_text(json.dumps(document))
    return document


def independent_game():
    return {'slug': 'standalone', 'name': 'Standalone', 'description': 'Original platformer',
            'created': '2026-10-12', 'game_version': '0.1.0', 'kind': 'native game',
            'presentation': '2d', 'networking': 'offline',
            'source_url': 'https://github.com/owner/engine/tree/' + 'b' * 40 + '/games/standalone',
            'install_help': 'Run a newer installer to update; saves are preserved.',
            'published_release': {'tag_name': 'v0.1.0', 'published_at': '2026-10-12T00:00:00Z',
                                  'assets': [{'name': 'standalone-setup-windows-x64.exe', 'size': 1234,
                                              'digest': 'sha256:' + 'a' * 64,
                                              'browser_download_url': 'https://github.com/owner/standalone/releases/download/v0.1.0/standalone-setup-windows-x64.exe'}]}}


class Downloads(unittest.TestCase):
    def test_uncommitted_ai_game_has_its_design_credit_before_the_publication_commit(self):
        from native_catalog import design_origin
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            wiki_fixture(root, 'new-ai-game', 'ai')
            self.assertEqual(design_origin(root, {'slug': 'new-ai-game', 'name': 'New AI Game'}), 'ai')

    def test_design_credit_uses_game_specific_ideaforge_publication_not_ai_code_authorship(self):
        from native_catalog import design_origin
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            def git(*args):
                return subprocess.run(['git', *args], cwd=root, check=True, capture_output=True, text=True)
            git('init')
            git('config', 'user.name', 'IdeaForge')
            git('config', 'user.email', 'idea-forge@users.noreply.github.com')
            (root / 'games/human').mkdir(parents=True)
            (root / 'games/human/README.md').write_text('Human concept, AI implementation')
            git('add', '.')
            git('commit', '-m', 'Implement a human-designed game with AI')
            human = {'slug': 'human', 'name': 'Human'}
            automatic = {'slug': 'automatic', 'name': 'Automatic'}
            self.assertEqual(design_origin(root, human), 'human')
            (root / 'site/thumbs').mkdir(parents=True)
            (root / 'site/thumbs/automatic.png').write_bytes(screenshot())
            (root / 'games/human/README.md').write_text('Unrelated source sync in the same export')
            git('add', '.')
            git('commit', '-m', 'Publish Automatic generated by IdeaForge')
            self.assertEqual(design_origin(root, automatic), 'ai')
            self.assertEqual(design_origin(root, human), 'human')
            self.assertEqual(design_origin(root, automatic | {'design_origin': 'human'}), 'human')
            with self.assertRaisesRegex(ValueError, 'Invalid design origin'):
                design_origin(root, {'slug': 'automatic', 'design_origin': 'guess'})

    def test_every_release_game_has_a_landscape_screenshot(self):
        root = Path(__file__).resolve().parent.parent
        definitions = json.loads((root / '.release-games.json').read_text())
        for game in definitions['data_playables'] + definitions['native_playables']:
            with self.subTest(game=game['slug']):
                site.game_screenshot(root / 'site/thumbs', root / 'games', game['slug'])
        from released_games import load
        for game in load(root):
            with self.subTest(game=game['slug']):
                site.game_screenshot(root / 'site/thumbs', root / 'games', game['slug'])

    def test_independent_release_joins_catalog_with_its_own_installer_and_source(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for directory in ('site', 'games', 'thumbs'):
                (root / directory).mkdir()
            for slug in ('bundled', 'standalone'):
                (root / f'thumbs/{slug}.png').write_bytes(screenshot())
                wiki_fixture(root, slug, 'ai' if slug == 'standalone' else 'human')
            game = independent_game()
            game['design_origin'] = 'ai'
            previous = copy.deepcopy(game['published_release'])
            previous['tag_name'] = 'v0.0.9'
            previous['assets'][0]['browser_download_url'] = previous['assets'][0]['browser_download_url'].replace('/v0.1.0/', '/v0.0.9/')
            game['previous_releases'] = [previous]
            (root / 'site/released-games.json').write_text(json.dumps({'version': 1, 'games': [game]}))
            with (root / 'catalog.tsv').open('w', newline='') as out:
                writer = csv.DictWriter(out, fieldnames=['slug', 'name', 'release'], delimiter='\t')
                writer.writeheader()
                writer.writerow({'slug': 'bundled', 'name': 'Bundled', 'release': 'bundle-tag'})
            current = release('bundle-tag', ['bundled-setup-windows-x64.exe'])
            (root / 'release.json').write_text(json.dumps(current))
            site.build(SimpleNamespace(catalog=root / 'catalog.tsv', release_json=root / 'release.json',
                                      games_dir=root / 'games', thumbs=root / 'thumbs', out=root / 'out'))
            index = (root / 'out/index.html').read_text()
            self.assertIn('<strong>2 free games</strong>', index)
            self.assertIn('data-id="standalone" data-presentation="2d" data-networking="offline"', index)
            self.assertIn('href="games/standalone/"', index)
            self.assertIn('id="design-origin"', index)
            self.assertIn('data-design-origin="ai"', index)
            self.assertIn('data-design-origin="human"', index)
            self.assertIn('AI-designed</span>', index)
            self.assertIn('Human-designed</span>', index)
            self.assertIn('a block moves only when the square beyond it is empty.', index)
            self.assertIn('https://example.com/bundle-tag/bundled-setup-windows-x64.exe', index)
            page = (root / 'out/games/standalone/index.html').read_text()
            self.assertIn(game['published_release']['assets'][0]['browser_download_url'], page)
            self.assertIn(game['source_url'], page)
            self.assertIn(game['install_help'], page)
            self.assertIn('AI-designed</span>', page)
            self.assertIn('Game concept and rules generated by IdeaForge.', page)
            self.assertIn('a' * 64, page)
            self.assertIn('<code>v0.1.0</code>', page)
            self.assertIn('<code>v0.0.9</code>', page)
            self.assertIn(previous['assets'][0]['browser_download_url'], page)
            self.assertEqual(page.count('Latest</span>'), 1)
            self.assertIn('Latest</span>', page)
            self.assertNotIn('Check for updates', page)
            self.assertEqual((root / 'out/thumbs/standalone.png').read_bytes(), screenshot())

    def test_independent_release_rejects_unpublished_or_unpinned_downloads(self):
        from released_games import load
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'site').mkdir()
            for field, value in [('draft', True), ('tag_name', 'wrong-tag'), ('url', 'https://github.com/owner/standalone/releases/latest/download/standalone-setup-windows-x64.exe'),
                                 ('digest', ''), ('source_url', 'https://github.com/owner/engine/tree/main/games/standalone')]:
                with self.subTest(field=field):
                    game = copy.deepcopy(independent_game())
                    if field == 'source_url':
                        game[field] = value
                    elif field in ('url', 'digest'):
                        game['published_release']['assets'][0]['browser_download_url' if field == 'url' else field] = value
                    else:
                        game['published_release'][field] = value
                    (root / 'site/released-games.json').write_text(json.dumps({'version': 1, 'games': [game]}))
                    with self.assertRaises(ValueError):
                        load(root)

    def test_missing_invalid_and_icon_screenshots_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            thumb = root / 'game.png'
            with self.assertRaisesRegex(ValueError, 'Missing game screenshot'):
                site.game_screenshot(root, root / 'games', 'game')
            thumb.write_bytes(b'not a PNG')
            with self.assertRaisesRegex(ValueError, 'must be a PNG'):
                site.game_screenshot(root, root / 'games', 'game')
            thumb.write_bytes(screenshot(256, 256))
            with self.assertRaisesRegex(ValueError, 'must be landscape'):
                site.game_screenshot(root, root / 'games', 'game')
            thumb.write_bytes(screenshot())
            icon = root / 'games/game/assets/icon.png'
            icon.parent.mkdir(parents=True)
            icon.write_bytes(thumb.read_bytes())
            with self.assertRaisesRegex(ValueError, 'launcher icon'):
                site.game_screenshot(root, root / 'games', 'game')

    def test_archive_names_identify_versions_and_escape_release_text(self):
        archive = release('immutable-archive', ['feta-setup-windows-x64.exe'],
                          name='Feta 0.2.0-playtest.1 <archived>')
        rows = site.version_rows([archive], 'feta', 'new')
        self.assertIn('Feta 0.2.0-playtest.1 &lt;archived&gt;', rows)
        self.assertIn('<code>immutable-archive</code>', rows)
        self.assertIn('https://example.com/immutable-archive/feta-setup-windows-x64.exe', rows)
        self.assertNotIn('<archived>', rows)

    def test_installer_preferred_and_actual_pinned_urls(self):
        current = release('new', ['game-windows-x64.zip', 'game-setup-windows-x64.exe'])
        url, label, _ = site.download_for(current, 'game')
        self.assertEqual(url, 'https://example.com/new/game-setup-windows-x64.exe')
        self.assertEqual(label, 'Download installer (.exe)')
        self.assertEqual(site.download_for(current, 'missing')[0], '')
        self.assertEqual(site.download_for(release('old', ['game-windows-x64.zip']), 'game')[0], '')

    def test_legacy_history_never_offers_zip_downloads(self):
        versions = [release('new', ['game-windows-x64.zip']),
                    release('old', ['game-windows-x64.zip', 'SHA256SUMS.txt']),
                    release('draft', ['game-windows-x64.zip'], draft=True),
                    release('preview', ['game-windows-x64.zip'], prerelease=True),
                    release('unrelated', ['another-windows-x64.zip'])]
        history = site.version_rows(versions[1:], 'game', 'new')
        self.assertEqual(history.count('<tr>'), 1)
        self.assertIn('Installer not yet available', history)
        self.assertNotIn('game-windows-x64.zip', history)
        self.assertNotIn('https://example.com/old/SHA256SUMS.txt', history)
        self.assertNotIn('/new/', history)
        self.assertNotIn('/draft/', history)
        self.assertNotIn('/preview/', history)
        self.assertNotIn('/unrelated/', history)

    def test_complete_site_uses_release_assets_not_mutable_catalog_links(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'games').mkdir()
            (root / 'thumbs').mkdir()
            (root / 'thumbs/game.png').write_bytes(screenshot())
            wiki_fixture(root, 'game')
            rows = [{'slug': 'game', 'name': 'Game café ✨', 'description': '<unsafe>', 'created': '2026-01-01',
                     'game_version': '0.1.0', 'kind': 'native', 'asset': 'https://bad/latest.zip'}]
            with (root / 'catalog.tsv').open('w', encoding='utf-8', newline='') as out:
                writer = csv.DictWriter(out, fieldnames=rows[0].keys(), delimiter='\t')
                writer.writeheader(); writer.writerows(rows)
            current = release('current', ['game-setup-windows-x64.exe', 'SHA256SUMS.txt', 'INSTALLER-SHA256SUMS.txt'])
            old = release('old', ['game-windows-x64.zip', 'game-setup-windows-x64.exe'])
            (root / 'release.json').write_text(json.dumps(current))
            (root / 'releases.json').write_text(json.dumps([current, old]))
            with patch.object(site, 'game_added_date', return_value='2026-01-01'):
                # Simulate output left by the previous browser-enabled builder.
                stale = root / 'out/web/old-game'
                stale.mkdir(parents=True)
                (stale / 'game.wasm').write_bytes(b'old runtime')
                old_page = root / 'out/games/browser-only'
                old_page.mkdir(parents=True)
                (old_page / 'index.html').write_text('Play in browser')
                site.build(SimpleNamespace(catalog=root / 'catalog.tsv', release_json=root / 'release.json',
                                          releases_json=root / 'releases.json', games_dir=root / 'games',
                                          thumbs=root / 'thumbs', out=root / 'out'))
            page = (root / 'out/index.html').read_text(encoding='utf-8')
            self.assertIn('<strong>1 free game</strong> for Windows.', page)
            self.assertIn('Game café ✨', page)
            self.assertFalse((root / 'out/web').exists())
            self.assertFalse(old_page.exists())
            self.assertNotIn('Play / Install', page)
            self.assertNotIn('data-browser', page)
            self.assertNotIn('id="distribution"', page)
            self.assertIn('data-star="game"', page)
            self.assertIn('Windows x64 only.', page)
            self.assertIn('https://example.com/current/game-setup-windows-x64.exe', page)
            self.assertNotIn('https://example.com/old/game-windows-x64.zip', page)
            self.assertIn('href="games/game/"', page)
            self.assertIn('src="thumbs/game.png"', page)
            self.assertIn('in-game screenshot', page)
            self.assertNotIn('thumb icon', page)
            self.assertNotIn('thumb tile', page)
            self.assertIn('https://example.com/current/INSTALLER-SHA256SUMS.txt', page)
            game_page = (root / 'out/games/game/index.html').read_text(encoding='utf-8')
            self.assertIn('https://example.com/old/game-setup-windows-x64.exe', game_page)
            self.assertNotIn('game-windows-x64.zip', game_page)
            self.assertIn('https://example.com/current/game-setup-windows-x64.exe', game_page)
            self.assertIn('Latest</span>', game_page)
            self.assertIn('src="../../thumbs/game.png"', game_page)
            self.assertNotIn('<details', game_page)
            self.assertEqual(game_page.count('<tbody>'), 1)
            self.assertIn('&lt;unsafe&gt;', page)
            self.assertNotIn('https://bad/', page)
            self.assertNotIn('BlueEngine Launcher', page)
            self.assertNotIn('id="missing"', page)
            previous = {str(p.relative_to(root / 'out')): p.read_bytes()
                        for p in (root / 'out').rglob('*') if p.is_file()}
            (root / 'thumbs/game.png').unlink()
            with self.assertRaisesRegex(ValueError, 'Missing game screenshot'):
                site.build(SimpleNamespace(catalog=root / 'catalog.tsv', release_json=root / 'release.json',
                                          releases_json=root / 'releases.json', games_dir=root / 'games',
                                          thumbs=root / 'thumbs', out=root / 'out'))
            self.assertEqual(previous, {str(p.relative_to(root / 'out')): p.read_bytes()
                                       for p in (root / 'out').rglob('*') if p.is_file()})
            (root / 'thumbs/game.png').write_bytes(screenshot())
            with (root / 'catalog.tsv').open('a', encoding='utf-8', newline='') as out:
                out.write('missing\tMissing\t\t\t\t\t\n')
            previous = {str(p.relative_to(root / 'out')): p.read_bytes()
                        for p in (root / 'out').rglob('*') if p.is_file()}
            with self.assertRaisesRegex(ValueError, 'no installer.*missing'):
                site.build(SimpleNamespace(catalog=root / 'catalog.tsv', release_json=root / 'release.json',
                                          releases_json=root / 'releases.json', games_dir=root / 'games',
                                          thumbs=root / 'thumbs', out=root / 'out'))
            self.assertEqual(previous, {str(p.relative_to(root / 'out')): p.read_bytes()
                                       for p in (root / 'out').rglob('*') if p.is_file()})

    def test_deployment_audit_rejects_web_payloads_and_download_links(self):
        from check import check
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            index = root / 'index.html'
            index.write_text('<a class="dl" href="https://example.com/game.exe">Download</a>')
            check(root)
            for link in ('web/game/', 'https://example.com/game.zip', 'game.wasm', 'game.html'):
                index.write_text(f'<a class="dl" href="{link}">Play</a>')
                with self.assertRaises(ValueError):
                    check(root)
            index.write_text('<canvas></canvas>')
            with self.assertRaises(ValueError):
                check(root)
            for preview in ('<img class="thumb icon" src="icon.png">', '<div class="thumb tile">G</div>'):
                index.write_text(preview)
                with self.assertRaisesRegex(ValueError, 'must use screenshots'):
                    check(root)
            index.write_text('Windows downloads')
            for filename in ('game.wasm', 'app.webmanifest', 'service-worker.js', 'web/thumbnail.png'):
                file = root / filename
                file.parent.mkdir(exist_ok=True)
                file.touch()
                with self.assertRaises(ValueError):
                    check(root)
                file.unlink()
                if file.parent != root:
                    file.parent.rmdir()


if __name__ == '__main__':
    unittest.main()
