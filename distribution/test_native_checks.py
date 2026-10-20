import json
from pathlib import Path
import tempfile
import unittest

from native_checks import DEFAULT_GAMES, ENGINE_REPOSITORY, main, select, write_outputs


class NativeGameSelection(unittest.TestCase):
    def fixture(self, root):
        definitions = []
        for slug in (*DEFAULT_GAMES, 'new-remix', 'plain-cargo'):
            game = root / 'games' / slug
            game.mkdir(parents=True)
            (game / 'Cargo.toml').write_text('[package]\nname="sample"\nversion="0.1.0"\n')
            kind = 'cargo' if slug == 'plain-cargo' else 'cargo-package'
            if kind == 'cargo-package':
                (game / 'scripts').mkdir()
                (game / 'scripts/ship.py').write_text('print("package fixture")\n')
            definitions.append(dict(slug=slug, name=slug, directory=f'games/{slug}', kind=kind, binary=slug))
        (root / '.games-catalog.json').write_text(json.dumps(dict(playables=[], source_repository=ENGINE_REPOSITORY,
                                                                 source_revision='a' * 40)))
        manifest = dict(version=1, game_roots=['games'], native_playables=definitions,
                        data_playables=[], non_playable_directories=[])
        (root / '.release-games.json').write_text(json.dumps(manifest))
        return manifest

    def test_default_retains_existing_nine_while_explicit_new_fork_runs_only_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            default = select(root)
            self.assertEqual([game['slug'] for game in default['games']], list(DEFAULT_GAMES))
            self.assertEqual(len(default['games']), 9)
            remix = select(root, 'new-remix')
            self.assertEqual(remix['games'], [dict(slug='new-remix', directory='games/new-remix', ship=True, hub=False)])
            self.assertEqual(remix['engine_revision'], 'a' * 40)

    def test_plain_cargo_skips_ship_and_forked_hub_test_keeps_real_room_gate(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            self.assertFalse(select(root, 'plain-cargo')['games'][0]['ship'])
            (root / 'games/new-remix/tests').mkdir()
            (root / 'games/new-remix/tests/engine_hub.rs').touch()
            self.assertTrue(select(root, 'new-remix')['games'][0]['hub'])

    def test_shell_payloads_unknown_games_and_escaping_paths_fail_before_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = self.fixture(root)
            for requested in ('../plain-cargo', 'plain-cargo\ngames=[]', '$(touch stolen)', 'all; false', 'Plain-Cargo', 'not-registered'):
                with self.subTest(requested=requested), self.assertRaises(ValueError):
                    select(root, requested)
            output = root / 'github-output'
            with self.assertRaises(SystemExit):
                main(['--root', str(root), '--game-slug', 'plain-cargo\nengine_revision=main', '--github-output', str(output)])
            self.assertFalse(output.exists())
            manifest['native_playables'][-1]['directory'] = 'games/new-remix'
            (root / '.release-games.json').write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):
                select(root, 'plain-cargo')

    def test_missing_packaging_unsafe_source_and_mutable_engine_pin_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            pin = root / '.games-catalog.json'
            original = pin.read_text()
            pin.write_text(original.replace('a' * 40, 'main'))
            with self.assertRaisesRegex(ValueError, 'immutable upstream'):
                select(root, 'plain-cargo')
            pin.write_text(original)
            (root / 'games/new-remix/scripts/ship.py').unlink()
            with self.assertRaisesRegex(ValueError, 'ship.py'):
                select(root, 'new-remix')

    def test_non_cargo_and_symlinked_manifests_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = self.fixture(root)
            manifest['native_playables'][-1]['kind'] = 'engine-sandbox'
            (root / '.release-games.json').write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, 'registered cargo'):
                select(root, 'plain-cargo')
            manifest['native_playables'][-1]['kind'] = 'cargo'
            (root / '.release-games.json').write_text(json.dumps(manifest))
            cargo = root / 'games/new-remix/Cargo.toml'
            cargo.unlink()
            cargo.symlink_to(root / 'games/plain-cargo/Cargo.toml')
            with self.assertRaisesRegex(ValueError, 'Symlinked'):
                select(root, 'new-remix')

    def test_output_is_round_trippable_matrix_json_and_actual_legacy_slug_is_supported(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            selection = select(root, 'plain-cargo')
            output = root / 'github-output'
            write_outputs(output, selection)
            lines = dict(line.split('=', 1) for line in output.read_text().splitlines())
            self.assertEqual(json.loads(lines['games']), selection['games'])
            self.assertEqual(lines['engine_revision'], 'a' * 40)
            self.assertEqual(len(lines), 2)
        root = Path(__file__).resolve().parent.parent
        spooky = select(root, 'spooky-kart')
        self.assertEqual([game['slug'] for game in spooky['games']], ['spooky-kart'])
        self.assertTrue(spooky['games'][0]['ship'])
        self.assertFalse(select(root, 'bluedm')['games'][0]['ship'])


if __name__ == '__main__':
    unittest.main()
