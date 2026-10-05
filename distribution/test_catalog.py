import hashlib
import io
import json
from pathlib import Path
import shutil
import tempfile
import unittest
import zipfile

from check_catalog import check
from fetch_release import stage


class Coverage(unittest.TestCase):
    def test_unlisted_game_and_duplicate_download_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'games/sample').mkdir(parents=True)
            (root / 'games/sample/Cargo.toml').touch()
            (root / '.games-catalog.json').write_text('{"playables": []}')
            manifest = dict(version=1, game_roots=['games'], data_playables=[],
                            native_playables=[dict(slug='sample', name='Sample', directory='games/sample',
                                                   kind='cargo', binary='sample')], non_playable_directories=[])
            path = root / '.release-games.json'
            path.write_text(json.dumps(manifest))
            self.assertEqual(check(root), {'sample'})
            (root / 'games/forgotten').mkdir()
            with self.assertRaisesRegex(ValueError, 'no Windows download.*forgotten'):
                check(root)
            shutil.rmtree(root / 'games/forgotten')
            manifest['native_playables'] *= 2
            path.write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, 'duplicate.*sample'):
                check(root)


class PinnedRelease(unittest.TestCase):
    def archive(self, extra=None):
        output = io.BytesIO()
        with zipfile.ZipFile(output, 'w') as bundle:
            bundle.writestr('Game.exe', b'MZpublic fixture')
            bundle.writestr('ignored.txt', b'not declared')
            if extra:
                bundle.writestr(extra, b'bad')
        return output.getvalue()

    def source(self, data):
        return dict(sha256=hashlib.sha256(data).hexdigest(), files=['Game.exe'], executable='Game.exe')

    def test_checksum_and_archive_paths_fail_before_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / 'package'
            data = self.archive()
            source = self.source(data)
            with self.assertRaisesRegex(ValueError, 'SHA256'):
                stage(source, data + b'tampered', destination)
            self.assertFalse(destination.exists())
            for name in ('../outside', 'C:/outside', 'Game.EXE'):
                bad = self.archive(name)
                with self.assertRaisesRegex(ValueError, 'Unsafe or duplicate'):
                    stage(self.source(bad), bad, destination)
                self.assertFalse(destination.exists())
            stage(source, data, destination)
            self.assertEqual([p.name for p in destination.iterdir()], ['Game.exe'])


if __name__ == '__main__':
    unittest.main()
