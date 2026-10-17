import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile


@unittest.skipUnless(shutil.which('pwsh'), 'PowerShell is required for native release packaging')
class NativePackaging(unittest.TestCase):
    def test_exported_tooling_uses_catalog_engine_and_restores_environment(self):
        source = Path(__file__).resolve().parent.parent
        for failure in (False, True):
            for previous in (None, 'previous-tooling-root'):
                with self.subTest(failure=failure, previous=previous), tempfile.TemporaryDirectory() as temporary:
                    base = Path(temporary)
                    root, engine = base / 'games-repository', base / 'BlueEngine'
                    scripts = root / '.github/scripts'
                    game = root / 'games/sample'
                    scripts.mkdir(parents=True)
                    (root / 'distribution').mkdir()
                    (game / 'scripts').mkdir(parents=True)
                    (root / 'dist').mkdir()
                    engine.mkdir()
                    (engine / 'Cargo.toml').touch()
                    shutil.copy2(source / '.github/scripts/package_native_releases.ps1', scripts)
                    shutil.copy2(source / 'distribution/check_catalog.py', root / 'distribution')
                    (root / '.games-catalog.json').write_text(json.dumps({'playables': []}))
                    manifest = dict(version=1, game_roots=['games'], data_playables=[],
                                    native_playables=[dict(slug='sample', name='Sample', directory='games/sample',
                                                           kind='cargo-package', binary='sample')],
                                    non_playable_directories=[])
                    (root / '.release-games.json').write_text(json.dumps(manifest))
                    original_manifest = 'vesper3d = { package = "be2", path = "../../.." }\n'
                    (game / 'Cargo.toml').write_text(original_manifest)
                    (game / 'scripts/ship.py').write_text(
                        'import os\nfrom pathlib import Path\n'
                        'engine = Path(os.environ["BE2_TOOLING_ROOT"])\n'
                        'assert engine.is_absolute()\n'
                        'assert engine == Path(os.environ["EXPECTED_ENGINE"])\n'
                        'Path("tooling-observed.txt").write_text(str(engine))\n'
                        'if os.environ["FAIL_PACKAGE"] == "true":\n'
                        '    raise SystemExit(7)\n'
                        'Path("dist").mkdir(exist_ok=True)\n'
                        'Path("dist/sample.exe").write_bytes(b"MZfixture")\n')
                    harness = base / 'run.ps1'
                    harness.write_text('''$ErrorActionPreference = 'Stop'
$before = $env:BE2_TOOLING_ROOT
$failed = $false
try {
    & $env:PACKAGING_SCRIPT -EngineRoot $env:EXPECTED_ENGINE
} catch {
    $failed = $true
    if ($_.Exception.Message -notlike '*Sample packaging failed with exit code 7*') { throw }
}
if ($failed -ne ($env:FAIL_PACKAGE -eq 'true')) { throw 'Unexpected packaging outcome' }
if ($env:BE2_TOOLING_ROOT -cne $before) { throw 'Packaging changed the caller tooling root' }
exit 0
''')
                    env = os.environ.copy()
                    env.pop('BE2_TOOLING_ROOT', None)
                    if previous is not None:
                        env['BE2_TOOLING_ROOT'] = previous
                    env.update(EXPECTED_ENGINE=str(engine), FAIL_PACKAGE=str(failure).lower(),
                               PACKAGING_SCRIPT=str(scripts / 'package_native_releases.ps1'),
                               RUNNER_TEMP=str(base))
                    completed = subprocess.run(['pwsh', '-NoProfile', '-File', str(harness)],
                                               env=env, capture_output=True, text=True, timeout=60)
                    self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
                    self.assertEqual((game / 'tooling-observed.txt').read_text(), str(engine))
                    self.assertEqual((game / 'Cargo.toml').read_text(), original_manifest)
                    archive = root / 'dist/sample-windows-x64.zip'
                    if failure:
                        self.assertFalse(archive.exists())
                    else:
                        with zipfile.ZipFile(archive) as package:
                            self.assertEqual(package.read('sample.exe'), b'MZfixture')


if __name__ == '__main__':
    unittest.main()
