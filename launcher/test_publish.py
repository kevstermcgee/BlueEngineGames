import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from publish import ASSETS, INSTALLER, METADATA, GitHub, PublicationError, command, publish, publish_metadata, verify_release

REPOSITORY = 'kevstermcgee/BlueEngineGames'
VERSION = '0.1.10'


def metadata(version=VERSION):
    return dict(version=version, url=f'https://github.com/{REPOSITORY}/releases/download/launcher-v{version}/{INSTALLER}',
                sha256='a' * 64, size=123, update={'version': version})


def release_files(installer=b'MZoriginal immutable installer'):
    digest = hashlib.sha256(installer).hexdigest()
    update = dict(version=VERSION, pub_date='2026-10-20T14:00:00Z',
                  platforms={'windows-x86_64': dict(url=metadata()['url'], signature='original-signature')})
    return {INSTALLER: installer, INSTALLER + '.sig': b'original-signature\n',
            'launcher-update.json': json.dumps(update).encode(),
            'LAUNCHER-SHA256SUMS.txt': f'{digest}  {INSTALLER}\n'.encode()}


class FakeGitHub:
    repository = REPOSITORY

    def __init__(self, source, files=None):
        self.source, self.files = source, files
        self.created, self.downloaded = 0, 0

    def release(self, tag):
        if self.files is None:
            return None
        return dict(tag_name=tag, draft=False, prerelease=False,
                    html_url=f'https://github.com/{REPOSITORY}/releases/tag/{tag}',
                    assets=[dict(name=name, state='uploaded', size=len(data), digest='sha256:' + hashlib.sha256(data).hexdigest(),
                                 browser_download_url=f'https://github.com/{REPOSITORY}/releases/download/{tag}/{name}')
                            for name, data in self.files.items()])

    def tag_commit(self, tag, *, missing=False):
        return self.source

    def download(self, tag, directory):
        self.downloaded += 1
        for name, data in self.files.items():
            (directory / name).write_bytes(data)

    def create(self, tag, source, version, directory):
        self.created += 1
        self.source = source
        self.files = {name: (directory / name).read_bytes() for name in ASSETS}
        return False  # GitHub accepted the release, but the create response was lost.


def git(root, *args, check=True):
    return command(['git', '-c', 'user.name=Publisher test', '-c', 'user.email=publisher@example.test', *args], cwd=root, check=check)


def repository(root, remote=None):
    root.mkdir()
    git(root, 'init', '-b', 'main')
    (root / 'launcher/src-tauri').mkdir(parents=True)
    (root / 'launcher/package.json').write_text(json.dumps({'version': VERSION}))
    (root / 'launcher/src-tauri/tauri.conf.json').write_text('{}')
    (root / 'site').mkdir()
    (root / METADATA).write_text(json.dumps(metadata('0.1.9')))
    git(root, 'add', '.')
    git(root, 'commit', '-m', 'Initial source')
    git(root, 'remote', 'add', 'origin', str(remote) if remote else f'https://github.com/{REPOSITORY}.git')
    if remote:
        git(root, 'push', '-u', 'origin', 'main')
        git(remote, 'symbolic-ref', 'HEAD', 'refs/heads/main')
    return git(root, 'rev-parse', 'HEAD').stdout.strip()


class ImmutableLauncherRelease(unittest.TestCase):
    def test_same_source_retry_uses_original_assets_without_reading_rebuilt_bundle(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'source'
            source = repository(root)
            github = FakeGitHub(source, release_files())
            verified = []
            def signature_check(root, config, installer, signature, version):
                verified.append(installer.read_bytes())
            with patch('publish.publish_metadata', return_value={'metadata_commit': 'b' * 40, 'metadata_changed': False}) as feed:
                result = publish(root, REPOSITORY, source, root / 'nonexistent-rebuilt-bundle',
                                 github=github, signature_verifier=signature_check)
            self.assertTrue(result['reused_release'])
            self.assertEqual(github.created, 0)
            self.assertEqual(verified, [b'MZoriginal immutable installer'])
            self.assertEqual(feed.call_args.args[1]['sha256'], hashlib.sha256(verified[0]).hexdigest())

    def test_same_version_other_source_and_missing_or_mismatched_assets_fail_before_feed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for kind in ('different-source', 'missing-file', 'missing-digest', 'wrong-download', 'wrong-signature'):
                with self.subTest(kind=kind), tempfile.TemporaryDirectory(dir=root) as download:
                    github = FakeGitHub('a' * 40, release_files())
                    release = github.release('launcher-v' + VERSION)
                    source = 'b' * 40 if kind == 'different-source' else 'a' * 40
                    if kind == 'missing-file': release['assets'].pop()
                    if kind == 'missing-digest': release['assets'][0].pop('digest')
                    if kind == 'wrong-download': github.files[INSTALLER] = b'MZchanged after metadata read'
                    if kind == 'wrong-signature':
                        update = json.loads(github.files['launcher-update.json'])
                        update['platforms']['windows-x86_64']['signature'] = 'different-signature'
                        github.files['launcher-update.json'] = json.dumps(update).encode()
                        release = github.release('launcher-v' + VERSION)
                    with self.assertRaises(PublicationError):
                        verify_release(github, release, source, VERSION, Path(download), lambda *args: None)

    def test_existing_tag_for_other_source_cannot_create_a_new_release(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'source'
            source = repository(root)
            github = FakeGitHub('b' * 40)
            with patch('publish.publish_metadata') as feed, self.assertRaisesRegex(PublicationError, 'different source'):
                publish(root, REPOSITORY, source, root / 'no-bundle', github=github)
            self.assertEqual(github.created, 0)
            feed.assert_not_called()

    def test_lost_create_acknowledgement_recovers_without_overwriting_release_assets(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'source'
            source = repository(root)
            bundle = root / 'built'
            bundle.mkdir()
            (bundle / 'new.exe').write_bytes(b'MZcandidate')
            (bundle / 'new.exe.sig').write_text('original-signature\n')
            github = FakeGitHub(source)
            with patch('publish.publish_metadata', return_value={'metadata_commit': 'b' * 40, 'metadata_changed': True}):
                result = publish(root, REPOSITORY, source, bundle, github=github, signature_verifier=lambda *args: None)
            self.assertEqual(github.created, 1)
            self.assertEqual(github.downloaded, 1)
            self.assertEqual(github.files[INSTALLER], b'MZcandidate')
            self.assertFalse(result['reused_release'])

    def test_annotated_tag_is_resolved_and_never_trusted_as_mutable_target_commitish(self):
        github = GitHub(REPOSITORY)
        with patch.object(github, 'api', side_effect=[{'object': {'type': 'tag', 'sha': 'a' * 40}},
                                                    {'object': {'type': 'commit', 'sha': 'b' * 40}}]):
            self.assertEqual(github.tag_commit('launcher-v' + VERSION), 'b' * 40)


class WebsiteFeedRetry(unittest.TestCase):
    def setup_repositories(self, temporary):
        base = Path(temporary)
        remote = base / 'remote.git'
        git(base, 'init', '--bare', str(remote))
        root = base / 'source'
        source = repository(root, remote)
        other = base / 'other'
        git(base, 'clone', str(remote), str(other))
        return root, remote, other, source

    def test_non_fast_forward_retries_only_feed_and_preserves_concurrent_game_change(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, remote, other, source = self.setup_repositories(temporary)
            pushes = []
            def racing_run(args, **kwargs):
                if args[:2] == ['git', 'push']:
                    pushes.append(args)
                    if len(pushes) == 1:
                        (other / 'game-change.txt').write_text('Concurrent game source update')
                        git(other, 'add', 'game-change.txt')
                        git(other, 'commit', '-m', 'Concurrent source change')
                        git(other, 'push', 'origin', 'main')
                return command(args, **kwargs)
            result = publish_metadata(root, metadata(), run=racing_run)
            self.assertEqual(len(pushes), 2)
            self.assertEqual(json.loads(git(remote, 'show', 'main:' + METADATA).stdout), metadata())
            self.assertEqual(git(remote, 'show', 'main:game-change.txt').stdout, 'Concurrent game source update')
            self.assertEqual(git(root, 'rev-parse', 'HEAD').stdout.strip(), source)
            self.assertTrue(result['metadata_changed'])
            repeated = publish_metadata(root, metadata())
            self.assertFalse(repeated['metadata_changed'])
            self.assertEqual(repeated['metadata_commit'], result['metadata_commit'])

    def test_lost_push_acknowledgement_is_a_verified_noop_on_retry(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, remote, other, source = self.setup_repositories(temporary)
            pushes = 0
            def lost_ack(args, **kwargs):
                nonlocal pushes
                result = command(args, **kwargs)
                if args[:2] == ['git', 'push']:
                    pushes += 1
                    return subprocess.CompletedProcess(args, 1, '', 'connection lost after accepted push')
                return result
            result = publish_metadata(root, metadata(), run=lost_ack)
            self.assertEqual(pushes, 1)
            self.assertFalse(result['metadata_changed'])
            self.assertEqual(git(remote, 'rev-list', '--count', 'main').stdout.strip(), '2')

    def test_same_version_mismatch_and_newer_feed_are_never_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, remote, other, source = self.setup_repositories(temporary)
            desired = metadata('0.1.9')
            desired['sha256'] = 'b' * 64
            with self.assertRaisesRegex(PublicationError, 'different website metadata'):
                publish_metadata(root, desired)
            with self.assertRaisesRegex(PublicationError, 'newer launcher'):
                publish_metadata(root, metadata('0.1.8'))
            self.assertEqual(git(remote, 'rev-list', '--count', 'main').stdout.strip(), '1')

    def test_existing_invalid_metadata_is_preserved_even_when_json_value_is_falsey(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, remote, other, source = self.setup_repositories(temporary)
            for value in ([], None, {}, False, 0):
                with self.subTest(value=value):
                    (other / METADATA).write_text(json.dumps(value))
                    git(other, 'add', METADATA)
                    git(other, 'commit', '-m', 'Corrupted website metadata')
                    git(other, 'push', 'origin', 'main')
                    with self.assertRaises(PublicationError):
                        publish_metadata(root, metadata())
                    self.assertEqual(json.loads(git(remote, 'show', 'main:' + METADATA).stdout), value)


if __name__ == '__main__':
    unittest.main()
