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
        cases = [(dependency, failure, previous, 'catalog')
                 for dependency in ('path', 'git')
                 for failure in (False, True)
                 for previous in (None, 'previous-tooling-root')]
        cases.extend(('git', False, 'previous-tooling-root', metadata)
                     for metadata in ('wrong-path', 'git-source'))
        for dependency, failure, previous, metadata in cases:
            with self.subTest(dependency=dependency, failure=failure, previous=previous, metadata=metadata), tempfile.TemporaryDirectory() as temporary:
                base = Path(temporary).resolve()
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
                shutil.copy2(source / 'distribution/bind_engine.py', root / 'distribution')
                (root / '.games-catalog.json').write_text(json.dumps({'playables': []}))
                manifest = dict(version=1, game_roots=['games'], data_playables=[],
                                native_playables=[dict(slug='sample', name='Sample', directory='games/sample',
                                                       kind='cargo-package', binary='sample')],
                                non_playable_directories=[])
                (root / '.release-games.json').write_text(json.dumps(manifest))
                source_fields = ('path = "../../.."' if dependency == 'path' else
                                 'git = "https://github.com/kevstermcgee/BlueEngine.git", rev = "old-engine"')
                original_manifest = (
                    '[dependencies]\r\nvesper3d = { package = "be2", ' + source_fields
                    + ', version = "0.2", default-features = false, features = ["presentation", "physics"], optional = true }\r\n'
                ).encode()
                source_line = ('' if dependency == 'path' else
                               'source = "git+https://github.com/kevstermcgee/BlueEngine.git?rev=old-engine#old-engine"\r\n')
                original_lock = ('version = 4\r\n\r\n[[package]]\r\nname = "be2"\r\nversion = "0.2.0"\r\n'
                                 + source_line + 'dependencies = []\r\n').encode()
                (game / 'Cargo.toml').write_bytes(original_manifest)
                (game / 'Cargo.lock').write_bytes(original_lock)
                (game / 'scripts/ship.py').write_text(
                    'import json\nimport os\nfrom pathlib import Path\nimport tomllib\n'
                    'engine = Path(os.environ["BE2_TOOLING_ROOT"])\n'
                    'assert engine.is_absolute()\n'
                    'assert engine == Path(os.environ["EXPECTED_ENGINE"]), (str(engine), os.environ["EXPECTED_ENGINE"])\n'
                    'dependency = tomllib.loads(Path("Cargo.toml").read_text())["dependencies"]["vesper3d"]\n'
                    'expected = {"package": "be2", "version": "0.2", "default-features": False, "features": ["presentation", "physics"], "optional": True}\n'
                    'assert {k: v for k, v in dependency.items() if k != "path"} == expected, dependency\n'
                    'assert Path(dependency["path"]) == engine, dependency\n'
                    'locked_engine = tomllib.loads(Path("Cargo.lock").read_text())["package"][0]\n'
                    'assert "source" not in locked_engine, locked_engine\n'
                    'Path("dependency-observed.json").write_text(json.dumps(dependency))\n'
                    'Path("tooling-observed.txt").write_text(str(engine))\n'
                    'if os.environ["FAIL_PACKAGE"] == "true":\n'
                    '    raise SystemExit(7)\n'
                    'Path("dist").mkdir(exist_ok=True)\n'
                    'Path("dist/sample.exe").write_bytes(b"MZfixture")\n')
                harness = base / 'run.ps1'
                harness.write_text('''$ErrorActionPreference = 'Stop'
function global:cargo {
    $cargoArguments = @($args | ForEach-Object { [string]$_ })
    if ($cargoArguments.Count -ne 6 -or $cargoArguments[0] -ne 'metadata' -or $cargoArguments[1] -ne '--locked' -or
        $cargoArguments[2] -ne '--format-version' -or $cargoArguments[3] -ne '1' -or
        $cargoArguments[4] -ne '--manifest-path' -or $cargoArguments[5] -ne $env:EXPECTED_MANIFEST) {
        throw "Unexpected Cargo invocation in packaging fixture: $args"
    }
    [IO.File]::WriteAllText($env:METADATA_OBSERVED, (ConvertTo-Json -InputObject $cargoArguments -Compress))
    $engineManifest = Join-Path $env:EXPECTED_ENGINE 'Cargo.toml'
    $source = $null
    if ($env:METADATA_MODE -eq 'wrong-path') {
        $engineManifest = Join-Path $env:RUNNER_TEMP 'old-engine/Cargo.toml'
    } elseif ($env:METADATA_MODE -eq 'git-source') {
        $source = 'git+https://github.com/kevstermcgee/BlueEngine.git?rev=old-engine#old-engine'
    }
    $global:LASTEXITCODE = 0
    $enginePackage = @{ name = 'be2'; source = $source; manifest_path = $engineManifest } |
        ConvertTo-Json -Depth 5 -Compress
    '{"packages":[' + $enginePackage + ',{"name":"feature-case-fixture","features":{"USB":[],"usb":[]}}]}'
}
$before = $env:BE2_TOOLING_ROOT
$failed = $false
try {
    & $env:PACKAGING_SCRIPT -EngineRoot $env:EXPECTED_ENGINE
} catch {
    $failed = $true
    if ($env:METADATA_MODE -ne 'catalog') {
        if ($_.Exception.Message -notlike '*catalog engine*') { throw }
    } elseif ($_.Exception.Message -notlike '*Sample packaging failed with exit code 7*') { throw }
}
if ($failed -ne ($env:FAIL_PACKAGE -eq 'true' -or $env:METADATA_MODE -ne 'catalog')) {
    throw 'Unexpected packaging outcome'
}
if ($env:BE2_TOOLING_ROOT -cne $before) { throw 'Packaging changed the caller tooling root' }
exit 0
''')
                env = os.environ.copy()
                env.pop('BE2_TOOLING_ROOT', None)
                if previous is not None:
                    env['BE2_TOOLING_ROOT'] = previous
                env.update(EXPECTED_ENGINE=str(engine), FAIL_PACKAGE=str(failure).lower(),
                           PACKAGING_SCRIPT=str(scripts / 'package_native_releases.ps1'),
                           EXPECTED_MANIFEST=str(game / 'Cargo.toml'), METADATA_MODE=metadata,
                           METADATA_OBSERVED=str(game / 'metadata-observed.json'),
                           RUNNER_TEMP=str(base))
                completed = subprocess.run(['pwsh', '-NoProfile', '-File', str(harness)],
                                           env=env, capture_output=True, text=True, timeout=60)
                self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
                self.assertEqual((game / 'Cargo.toml').read_bytes(), original_manifest)
                self.assertEqual((game / 'Cargo.lock').read_bytes(), original_lock)
                metadata_arguments = json.loads((game / 'metadata-observed.json').read_text())
                self.assertEqual(metadata_arguments, ['metadata', '--locked', '--format-version', '1',
                                                      '--manifest-path', str(game / 'Cargo.toml')])
                archive = root / 'dist/sample-windows-x64.zip'
                if metadata != 'catalog':
                    self.assertFalse((game / 'tooling-observed.txt').exists())
                    self.assertFalse((game / 'dependency-observed.json').exists())
                    self.assertFalse(archive.exists())
                    continue
                self.assertEqual((game / 'tooling-observed.txt').read_text(), str(engine))
                observed = json.loads((game / 'dependency-observed.json').read_text())
                self.assertEqual(Path(observed['path']), engine)
                if failure:
                    self.assertFalse(archive.exists())
                else:
                    with zipfile.ZipFile(archive) as package:
                        self.assertEqual(package.read('sample.exe'), b'MZfixture')


if __name__ == '__main__':
    unittest.main()
