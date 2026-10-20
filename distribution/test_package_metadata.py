import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

from distribution import game_metadata as metadata
from distribution import package_metadata as adapter


def pe(certificate=b''):
    """Structured, non-runnable PE fixture; artifact inspection never executes it."""
    data = bytearray(1024)
    data[:2] = b'MZ'
    struct.pack_into('<I', data, 60, 128)
    data[128:132] = b'PE\0\0'
    struct.pack_into('<HH', data, 132, 0x8664, 1)
    struct.pack_into('<H', data, 148, 240)
    struct.pack_into('<H', data, 152, 0x20b)
    struct.pack_into('<I', data, 152 + 108, 16)
    data[392:400] = b'.text\0\0\0'
    struct.pack_into('<II', data, 408, 512, 512)
    if certificate:
        struct.pack_into('<II', data, 152 + 112 + 32, len(data), len(certificate))
        data.extend(certificate)
    return bytes(data)


def command(root, *args):
    return subprocess.check_output(['git', '-C', str(root), '-c', 'user.name=Fixture',
                                    '-c', 'user.email=fixture@invalid', *args], stderr=subprocess.DEVNULL).decode().strip()


class PackageMetadataTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='portable-package-')
        self.addCleanup(self.temp.cleanup)
        base = Path(self.temp.name).resolve()
        self.root, self.engine, self.stage = base / 'games', base / 'engine', base / 'stage'
        for root, name in ((self.root, 'Games'), (self.engine, 'Engine')):
            root.mkdir()
            command(root, 'init', '-q', '-b', 'main')
            command(root, 'remote', 'add', 'origin', f'https://github.com/public/{name}.git')
        self.stage.mkdir()
        self.game = self.root / 'games/sample'
        (self.game / 'assets').mkdir(parents=True)
        (self.game / 'Cargo.toml').write_text('[package]\nname="sample"\nversion="1.2.3"\nauthors=["Public Author <private@example.com>"]\nkeywords=["puzzle"]\n')
        (self.game / 'assets/identity.json').write_text(json.dumps(dict(title='Sample', tagline='Choose a route.', controls='WASD')))
        (self.game / 'wiki.json').write_text(json.dumps(dict(version=1, overview='A public overview.', controls=['WASD moves.', 'Space jumps.'], story='No further story.', rules=['Reach the goal.'])))
        (self.engine / 'Cargo.toml').write_text('[package]\nname="be2"\nversion="0.2.0"\n')
        (self.engine / 'games-publish.json').write_text(json.dumps(dict(collections=dict(games=[dict(source='games/sample', destination='sample')]))))
        shutil.copytree(self.game, self.engine / 'games/sample')
        command(self.engine, 'add', '.')
        command(self.engine, 'commit', '-qm', 'Engine source')
        self.catalog = dict(playables=[], files=[], source_repository='public/Engine', source_revision=command(self.engine, 'rev-parse', 'HEAD'))
        self.release = dict(data_playables=[], native_playables=[dict(slug='sample', name='Sample', directory='games/sample', kind='cargo', binary='sample')])
        self.commit()

    def commit(self):
        (self.root / '.games-catalog.json').write_text(json.dumps(self.catalog))
        (self.root / '.release-games.json').write_text(json.dumps(self.release))
        command(self.root, 'add', '.')
        command(self.root, 'commit', '-qm', 'Game source')

    def prepare(self):
        (self.stage / 'sample.exe').write_bytes(pe())
        (self.stage / 'Play-sample.bat').write_bytes(b'@echo off\r\nsample.exe %*\r\n')
        (self.stage / 'Launch Sample.cmd').write_bytes(b'@echo off\r\nsample.exe --fullscreen %*\r\n')
        return adapter.stage_package(self.root, 'sample', self.stage, self.engine)

    def zip(self, name='game.zip'):
        archive = self.stage.parent / name
        with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED) as bundle:
            for path in self.stage.iterdir():
                bundle.write(path, path.name)
        adapter.archive_package(self.stage, archive)
        return archive

    def test_actual_source_identity_and_public_information(self):
        document = adapter.build_document(self.root, 'sample', self.engine)
        self.assertEqual(document['source'], dict(repository='public/Games', commit=command(self.root, 'rev-parse', 'HEAD'),
                                                 game_path='games/sample', tree=command(self.root, 'rev-parse', 'HEAD:games/sample')))
        self.assertEqual(document['engine']['commit'], self.catalog['source_revision'])
        self.assertEqual(document['game']['version'], '1.2.3')
        self.assertEqual(document['game']['controls'], 'WASD')
        self.assertEqual(document['wiki']['controls'], ['WASD moves.', 'Space jumps.'])
        self.assertEqual(document['game']['authors'], ['Public Author'])
        self.assertEqual(document['game']['tags'], ['puzzle'])
        self.assertEqual(document['wiki']['overview'], 'A public overview.')
        self.assertNotIn('private@example.com', metadata.canonical_payload(document).decode())

    def test_vendored_reader_is_the_reviewed_cross_repository_contract(self):
        self.assertEqual(hashlib.sha256(Path(metadata.__file__).read_bytes()).hexdigest(),
                         'e991033e5a1cc075166cd2a84363f4388debe29ad73617ea75236e29d0c318e6')

    def test_engine_managed_exports_use_original_source_not_companion(self):
        # Windows may materialize CRLF while the immutable exported Git blob is
        # LF. Build the catalog from the committed bytes, like the real exporter.
        command(self.root, 'config', 'core.autocrlf', 'true')
        path = self.game / 'Cargo.toml'
        path.write_bytes(path.read_bytes().replace(b'\r\n', b'\n').replace(b'\n', b'\r\n'))
        committed = subprocess.check_output(['git', '-C', str(self.root), 'show', 'HEAD:games/sample/Cargo.toml'])
        self.assertNotEqual(committed, (self.game / 'Cargo.toml').read_bytes())
        self.catalog['files'] = [dict(path='games/sample/Cargo.toml', sha256=hashlib.sha256(committed).hexdigest())]
        self.commit()
        document = adapter.build_document(self.root, 'sample', self.engine)
        self.assertEqual(document['source'], dict(repository='public/Engine', commit=self.catalog['source_revision'],
                                                 game_path='games/sample', tree=command(self.engine, 'rev-parse', 'HEAD:games/sample')))
        (self.game / 'Cargo.toml').write_text('[package]\nname="sample"\nversion="9.0.0"\n')
        command(self.root, 'add', '.')
        command(self.root, 'commit', '-qm', 'Unexported local edit')
        with self.assertRaisesRegex(ValueError, 'managed game changed'):
            adapter.build_document(self.root, 'sample', self.engine)

    def test_unchanged_game_folder_keeps_tree_hint_across_snapshot_commits(self):
        before = adapter.build_document(self.root, 'sample', self.engine)['source']
        (self.root / 'unrelated.txt').write_text('Unrelated repository update')
        command(self.root, 'add', '.')
        command(self.root, 'commit', '-qm', 'Advance source snapshot')
        after = adapter.build_document(self.root, 'sample', self.engine)['source']
        self.assertNotEqual(before['commit'], after['commit'])
        self.assertEqual(before['tree'], after['tree'])
        self.assertEqual(before['tree'], command(self.root, 'rev-parse', 'HEAD:games/sample'))

    def test_file_source_never_invents_a_folder_tree_hint(self):
        binding = dict(repository='public/Games', commit=command(self.root, 'rev-parse', 'HEAD'), game_path='games/sample/Cargo.toml')
        self.assertNotIn('tree', adapter.source_tree_binding(self.root, binding))

    def test_dirty_source_and_wrong_engine_never_get_a_publishable_pin(self):
        (self.game / 'Cargo.toml').write_text('uncommitted source')
        with self.assertRaisesRegex(ValueError, 'Commit the selected game'):
            adapter.build_document(self.root, 'sample', self.engine)
        command(self.root, 'restore', '.')
        self.catalog['source_revision'] = 'a' * 40
        self.commit()
        with self.assertRaisesRegex(ValueError, 'exact catalog Engine'):
            adapter.build_document(self.root, 'sample', self.engine)

    def test_modified_or_untracked_engine_source_cannot_claim_the_committed_pin(self):
        for path in (self.engine / 'Cargo.toml', self.engine / 'untracked.rs'):
            with self.subTest(path=path.name):
                before = path.read_bytes() if path.exists() else None
                path.write_text('unpublished engine input')
                with self.assertRaisesRegex(ValueError, 'Engine source changes'):
                    adapter.build_document(self.root, 'sample', self.engine)
                if before is None:
                    path.unlink()
                else:
                    path.write_bytes(before)

    def test_exe_bat_zip_and_updater_repack_keep_one_identical_document(self):
        document = self.prepare()
        archive = self.zip()
        original = archive.read_bytes()
        with mock.patch.object(subprocess, 'run', side_effect=AssertionError('inspection executed content')):
            self.assertEqual(metadata.extract(self.stage / 'sample.exe'), document)
            self.assertEqual(metadata.extract(self.stage / 'Play-sample.bat'), document)
            self.assertEqual(metadata.extract(self.stage / 'Launch Sample.cmd'), document)
            self.assertEqual(metadata.extract(archive), document)
        self.assertTrue((self.stage / 'Play-sample.bat').read_bytes().endswith(b'@echo off\r\nsample.exe %*\r\n'))
        self.assertTrue((self.stage / 'Launch Sample.cmd').read_bytes().endswith(b'@echo off\r\nsample.exe --fullscreen %*\r\n'))
        self.assertEqual(document['package']['batch_launchers'], ['Launch Sample.cmd', 'Play-sample.bat'])
        with zipfile.ZipFile(archive) as bundle:
            self.assertEqual(bundle.getinfo(adapter.MANIFEST).compress_type, zipfile.ZIP_STORED)
            bundle.extractall(self.stage / 'repacked')
        repacked = self.stage / 'repacked'
        adapter.restore_archive(archive, repacked)
        (repacked / 'Update-sample.exe').write_bytes(pe())
        with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED) as bundle:
            for path in repacked.iterdir():
                bundle.write(path, path.name)
        adapter.archive_package(repacked, archive)
        self.assertNotEqual(hashlib.sha256(archive.read_bytes()).hexdigest(), hashlib.sha256(original).hexdigest())
        self.assertEqual(metadata.extract(archive), document)
        self.assertEqual(metadata.extract(repacked / 'sample.exe'), document)

    def test_frozen_document_survives_intentional_temporary_sdk_binding(self):
        document = adapter.build_document(self.root, 'sample', self.engine)
        (self.game / 'Cargo.toml').write_text('temporary SDK binding')
        (self.stage / 'sample.exe').write_bytes(pe())
        result = adapter.stage_package(self.root, 'sample', self.stage, document=document)
        self.assertEqual(result['source']['commit'], document['source']['commit'])
        self.assertEqual(metadata.extract(self.stage / 'sample.exe'), result)

    def test_external_signed_original_is_unchanged_and_wrapper_marks_source_unavailable(self):
        self.release['native_playables'][0]['kind'] = 'release-asset'
        proof = dict(url='https://github.com/public/External/releases/download/v1/Game.zip', sha256='a' * 64,
                     version='2.0', executable='Original.exe', files=['Original.exe'])
        (self.game / 'release-source.json').write_text(json.dumps(proof))
        self.commit()
        original = pe(certificate=b'0123456789abcdef')
        (self.stage / 'Original.exe').write_bytes(original)
        with self.assertRaisesRegex(ValueError, 'newly compiled Play wrapper'):
            adapter.stage_package(self.root, 'sample', self.stage)
        (self.stage / 'Play-sample.exe').write_bytes(pe())
        document = adapter.stage_package(self.root, 'sample', self.stage)
        self.assertEqual((self.stage / 'Original.exe').read_bytes(), original)
        self.assertIsNone(document['source'])
        self.assertIsNone(document['engine'])
        self.assertEqual(document['posting']['source_status'], 'unavailable')
        self.assertEqual(document['package']['external_release'], proof)
        self.assertEqual(metadata.extract(self.stage / 'Play-sample.exe'), document)

    def test_legacy_preserve_archives_is_explicit_and_never_rewrites_original(self):
        archive = self.stage.parent / 'legacy.zip'
        with zipfile.ZipFile(archive, 'w') as bundle:
            bundle.writestr('Original.exe', pe())
        before = archive.read_bytes()
        with self.assertRaisesRegex(ValueError, 'rebuild it'):
            adapter.restore_archive(archive, self.stage)
        row = dict(slug='sample', name='Sample', game_version='1.0', release='historical')
        document = adapter.restore_archive(archive, self.stage, row)
        self.assertIsNone(document['source'])
        self.assertEqual(document['package']['archive_sha256'], hashlib.sha256(before).hexdigest())
        self.assertEqual(archive.read_bytes(), before)

    def test_final_installer_and_package_hash_receipts_cross_bind_the_same_game(self):
        document = self.prepare()
        archive = self.zip()
        dist = archive.parent
        archive = archive.rename(dist / 'sample-windows-x64.zip')
        installer = dist / 'sample-setup-windows-x64.exe'
        installer.write_bytes(pe() + b'fixture fresh Inno payload')
        result = subprocess.run([sys.executable, str(Path(adapter.__file__)), 'installer',
                                 '--stage', str(self.stage), '--installer', str(installer)],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        archive_hash, installer_hash = adapter.sha256_file(archive), adapter.sha256_file(installer)
        (dist / 'Games-catalog.tsv').write_text('slug\tgame_version\tsha256\n'
                                               f'sample\t1.2.3\t{archive_hash}\n')
        sums = dist / 'SHA256SUMS.txt'
        sums.write_text(f'{archive_hash}  {archive.name}\n{installer_hash}  {installer.name}\n')
        with mock.patch.object(subprocess, 'run', side_effect=AssertionError('artifact audit executed content')):
            proof = adapter.audit_dist(dist, installed=self.stage, slug='sample')
        self.assertEqual(proof['games'][0]['installer_sha256'], installer_hash)
        self.assertTrue(proof['games'][0]['installed'])
        self.assertEqual(metadata.extract(installer), document)
        sums.write_text(f'{archive_hash}  {archive.name}\n' + '0' * 64 + f'  {installer.name}\n')
        with self.assertRaisesRegex(ValueError, 'checksum was computed before'):
            adapter.audit_dist(dist)

    def test_signed_native_playable_is_never_rewritten(self):
        original = pe(certificate=b'0123456789abcdef')
        (self.stage / 'sample.exe').write_bytes(original)
        with self.assertRaises(metadata.MetadataError):
            adapter.stage_package(self.root, 'sample', self.stage, self.engine)
        self.assertEqual((self.stage / 'sample.exe').read_bytes(), original)

    def test_sandbox_only_stamps_its_explicit_owned_second_client(self):
        self.release['native_playables'][0]['kind'] = 'engine-sandbox'
        self.commit()
        for name in ('sample.exe', 'be2.exe', 'be2-tools.exe'):
            (self.stage / name).write_bytes(pe())
        (self.stage / 'Launch Sandbox.cmd').write_bytes(b'@echo off\r\nsample.exe\r\n')
        inventory = {path.name: adapter.sha256_file(path) for path in self.stage.iterdir()}
        (self.stage / 'SHA256.json').write_text(json.dumps(inventory))
        document = adapter.stage_package(self.root, 'sample', self.stage, self.engine)
        self.assertEqual(document['package']['playable_executables'], ['sample.exe', 'be2.exe'])
        self.assertEqual(metadata.extract(self.stage / 'be2.exe'), document)
        self.assertEqual((self.stage / 'be2-tools.exe').read_bytes(), pe())
        refreshed = json.loads((self.stage / 'SHA256.json').read_text())
        self.assertEqual(set(refreshed), set(inventory) | {adapter.MANIFEST})
        for name, digest in refreshed.items():
            self.assertEqual(digest, adapter.sha256_file(self.stage / name))
        self.assertEqual(refreshed['be2-tools.exe'], inventory['be2-tools.exe'])

    def test_ship_inventory_refresh_preserves_source_and_rejects_unchanged_corruption(self):
        (self.stage / 'sample.exe').write_bytes(pe())
        (self.stage / 'unchanged.png').write_bytes(b'original image fixture')
        stamp = dict(title='Sample', exe='sample.exe', files=['sample.exe', 'unchanged.png'],
                     file_sha256={name: adapter.sha256_file(self.stage / name) for name in ('sample.exe', 'unchanged.png')},
                     exe_sha256=adapter.sha256_file(self.stage / 'sample.exe'), review_source_sha256='a' * 64,
                     engine_revision=self.catalog['source_revision'], verified=dict(smoke=True, launch=True),
                     custom_receipt=dict(source='preserve this observation'))
        (self.stage / 'ship.json').write_text(json.dumps(stamp))
        (self.stage / 'unchanged.png').write_bytes(b'tampered copy')
        with self.assertRaisesRegex(ValueError, 'changed before metadata embedding'):
            adapter.stage_package(self.root, 'sample', self.stage, self.engine)
        self.assertEqual((self.stage / 'sample.exe').read_bytes(), pe())
        (self.stage / 'unchanged.png').write_bytes(b'original image fixture')
        adapter.stage_package(self.root, 'sample', self.stage, self.engine)
        refreshed = json.loads((self.stage / 'ship.json').read_text())
        self.assertEqual(set(refreshed['files']), set(stamp['files']) | {adapter.MANIFEST})
        for name in refreshed['files']:
            self.assertEqual(refreshed['file_sha256'][name], adapter.sha256_file(self.stage / name))
        self.assertEqual(refreshed['exe_sha256'], refreshed['file_sha256']['sample.exe'])
        self.assertEqual(refreshed['file_sha256']['unchanged.png'], stamp['file_sha256']['unchanged.png'])
        self.assertEqual(refreshed['review_source_sha256'], stamp['review_source_sha256'])
        self.assertEqual(refreshed['custom_receipt'], stamp['custom_receipt'])
        self.assertNotIn('verified', refreshed)
        self.assertTrue(refreshed['metadata_repack']['requires_native_review'])


if __name__ == '__main__':
    unittest.main()
