import copy
import json
import tempfile
import unittest
from pathlib import Path

from native_catalog import design_origin
from test_build import screenshot, wiki_fixture
from wiki import legacy_slugs, load_wiki, render_wiki, validate_wiki


class GameWiki(unittest.TestCase):
    def test_all_catalog_games_have_valid_guides_and_future_games_require_complete_artifacts(self):
        root = Path(__file__).resolve().parent.parent
        definitions = json.loads((root / '.release-games.json').read_text())
        rows = definitions['data_playables'] + definitions['native_playables']
        from released_games import load
        rows += load(root)
        self.assertEqual(len(legacy_slugs(root)), 50, 'The fixed migration roster must not expand for future games')
        for row in rows:
            with self.subTest(game=row['slug']):
                load_wiki(root, root / 'games', row, design_origin(root, row))

    def test_missing_new_game_guide_is_rejected_and_legacy_flag_cannot_bypass_requirements(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with self.assertRaisesRegex(ValueError, 'complete wiki.json'):
                load_wiki(root, root / 'games', {'slug': 'new'}, 'human')
            document = wiki_fixture(root, 'new')
            document['legacy'] = True
            document['gallery'] = document['gallery'][:1]
            (root / 'games/new/wiki.json').write_text(json.dumps(document))
            with self.assertRaisesRegex(ValueError, 'three distinct'):
                load_wiki(root, root / 'games', {'slug': 'new'}, 'human')

    def test_new_game_provenance_and_character_screenshots_are_mandatory(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            document = wiki_fixture(root, 'new')
            game = root / 'games/new'
            validate_wiki(game, document, 'human')
            missing = copy.deepcopy(document)
            missing['design']['original_prompt'] = None
            missing['design']['prompt_status'] = 'unavailable'
            with self.assertRaisesRegex(ValueError, 'original human creation prompt'):
                validate_wiki(game, missing, 'human')
            validate_wiki(game, missing, 'human', legacy=True)
            missing = copy.deepcopy(document)
            missing['design'] = {'origin': 'ai', 'rationale': 'A fun game'}
            with self.assertRaisesRegex(ValueError, 'public AI design rationale'):
                validate_wiki(game, missing, 'ai')
            missing = copy.deepcopy(document)
            missing['characters'] = [{'name': 'Mote', 'description': 'A small adventurer who travels through the world to recover a stolen light.'}]
            with self.assertRaisesRegex(ValueError, 'in-game screenshot'):
                validate_wiki(game, missing, 'human')

    def test_gallery_rejects_duplicates_traversal_missing_and_tiny_images(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            document = wiki_fixture(root, 'new')
            game = root / 'games/new'
            duplicate = copy.deepcopy(document)
            duplicate['gallery'][1]['image'] = duplicate['gallery'][0]['image']
            with self.assertRaisesRegex(ValueError, 'distinct'):
                validate_wiki(game, duplicate, 'human')
            for value in ('../private.png', '/private.png', 'assets/wiki/missing.png'):
                invalid = copy.deepcopy(document)
                invalid['gallery'][0]['image'] = value
                with self.assertRaises(ValueError):
                    validate_wiki(game, invalid, 'human')
            (game / 'assets/wiki/outside.png').symlink_to(root / 'outside.png')
            invalid['gallery'][0]['image'] = 'assets/wiki/outside.png'
            with self.assertRaises(ValueError):
                validate_wiki(game, invalid, 'human')
            (game / 'assets/wiki/tiny.png').write_bytes(screenshot(80, 80))
            invalid['gallery'][0]['image'] = 'assets/wiki/tiny.png'
            with self.assertRaisesRegex(ValueError, 'insufficient dimensions'):
                validate_wiki(game, invalid, 'human')

    def test_rendering_escapes_original_prompts_and_copies_real_images(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            document = wiki_fixture(root, 'new')
            document['design']['original_prompt'] = '<script>alert("prompt")</script> Create a new puzzle game about moving blocks.'
            destination = root / 'output'
            destination.mkdir()
            rendered = render_wiki(document, root / 'games/new', destination)
            self.assertNotIn('<script>', rendered)
            self.assertIn('&lt;script&gt;', rendered)
            for anchor in ('overview', 'rules', 'controls', 'story', 'characters', 'gallery', 'creation'):
                self.assertIn(f'id="{anchor}"', rendered)
            self.assertEqual(len(list((destination / 'images').glob('*.png'))), 3)


if __name__ == '__main__':
    unittest.main()
