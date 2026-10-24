"""Publish or recover one immutable, signed launcher release and its website feed."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

INSTALLER = 'BlueEngine-Games-Setup-x64.exe'
ASSETS = {INSTALLER: 512 * 1024**2, INSTALLER + '.sig': 65536,
          'launcher-update.json': 65536, 'LAUNCHER-SHA256SUMS.txt': 16384}
METADATA = 'site/launcher-release.json'
NOTES = ('Review text fields keep keyboard focus and composition during library refreshes. '
         'Feedback submission retries temporary connection and receiver failures with the same '
         'review ID, preserves drafts, and reports actionable errors.')


class PublicationError(ValueError):
    pass


def command(args, *, cwd=None, check=True, timeout=180):
    result = subprocess.run(args, cwd=cwd, capture_output=True, text=True, timeout=timeout,
                            env=dict(os.environ, GIT_TERMINAL_PROMPT='0'))
    if check and result.returncode:
        raise PublicationError(f'{args[0]} operation failed; the immutable release was not overwritten')
    return result


class GitHub:
    def __init__(self, repository):
        self.repository = repository

    def api(self, path, *, missing=False):
        result = command(['gh', 'api', f'repos/{self.repository}/{path}', '--include'], check=False)
        header, separator, body = result.stdout.replace('\r\n', '\n').partition('\n\n')
        match = re.match(r'HTTP/\S+ ([0-9]{3})', header)
        if not separator or not match:
            raise PublicationError('GitHub did not confirm the immutable release metadata')
        status = int(match[1])
        if status == 404 and missing:
            return None
        if result.returncode or status != 200:
            raise PublicationError('GitHub did not authorize or confirm the release lookup')
        value = json.loads(body)
        if not isinstance(value, dict):
            raise PublicationError('GitHub returned an invalid release object')
        return value

    def release(self, tag):
        return self.api(f'releases/tags/{tag}', missing=True)

    def tag_commit(self, tag, *, missing=False):
        reference = self.api(f'git/ref/tags/{tag}', missing=missing)
        if reference is None:
            return None
        for _ in range(5):
            obj = reference.get('object', {})
            if obj.get('type') == 'commit' and re.fullmatch(r'[0-9a-f]{40}', obj.get('sha', '')):
                return obj['sha']
            if obj.get('type') != 'tag' or not re.fullmatch(r'[0-9a-f]{40}', obj.get('sha', '')):
                break
            reference = self.api(f"git/tags/{obj['sha']}")
        raise PublicationError('The launcher tag does not resolve to an immutable source commit')

    def create(self, tag, source, version, directory):
        notes = directory / 'release-notes.txt'
        notes.write_text(NOTES + '\n', encoding='utf-8')
        return command(['gh', 'release', 'create', tag,
                        *(str(directory / name) for name in ASSETS), '--repo', self.repository,
                        '--target', source, '--title', f'BlueEngine Games Launcher {version}',
                        '--notes-file', str(notes), '--latest=false'], check=False).returncode == 0

    def download(self, tag, directory):
        command(['gh', 'release', 'download', tag, '--repo', self.repository, '--dir', str(directory),
                 *(part for name in ASSETS for part in ('--pattern', name))])


def version_tuple(version):
    if not isinstance(version, str) or not re.fullmatch(r'(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)', version):
        raise PublicationError('Launcher publication requires a stable three-part version')
    return tuple(map(int, version.split('.')))


def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024**2), b''):
            digest.update(chunk)
    return digest.hexdigest()


def verify_signature(root, config, installer, signature, version):
    result = command(['cargo', 'run', '--manifest-path', str(root / 'launcher/src-tauri/Cargo.toml'),
                      '--no-default-features', '--locked', '--example', 'verify-update-signature', '--',
                      str(config), str(installer), str(signature), version], cwd=root, check=False, timeout=900)
    if result.returncode:
        raise PublicationError('The immutable installer signature or signed version could not be verified')


def verify_release(github, release, source, version, directory, signature_check):
    tag = 'launcher-v' + version
    if (release.get('tag_name') != tag or release.get('draft') is not False
            or release.get('prerelease') is not False
            or release.get('html_url') != f'https://github.com/{github.repository}/releases/tag/{tag}'):
        raise PublicationError('The launcher release is not the expected public stable release')
    if github.tag_commit(tag) != source:
        raise PublicationError('This launcher version already identifies a different source commit; bump the version')
    assets = release.get('assets', [])
    if not isinstance(assets, list) or any(not isinstance(asset, dict) for asset in assets):
        raise PublicationError('GitHub returned invalid release assets')
    expected = {}
    for name, maximum in ASSETS.items():
        matches = [asset for asset in assets if asset.get('name') == name]
        if len(matches) != 1:
            raise PublicationError(f'The immutable launcher release is missing an unambiguous {name}')
        asset = matches[0]
        url = f'https://github.com/{github.repository}/releases/download/{tag}/{name}'
        size = asset.get('size')
        if (asset.get('state') != 'uploaded' or asset.get('browser_download_url') != url
                or type(size) is not int or not 0 < size <= maximum
                or not re.fullmatch(r'sha256:[0-9a-f]{64}', asset.get('digest') or '')):
            raise PublicationError(f'The immutable {name} lacks a pinned GitHub digest, URL or size')
        expected[name] = asset
    github.download(tag, directory)
    for name, asset in expected.items():
        path = directory / name
        if (path.is_symlink() or not path.is_file() or path.stat().st_size != asset['size']
                or sha256(path) != asset['digest'][7:]):
            raise PublicationError(f'The downloaded immutable {name} does not match GitHub metadata')
    installer = directory / INSTALLER
    with installer.open('rb') as executable:
        is_executable = executable.read(2) == b'MZ'
    if not is_executable:
        raise PublicationError('The immutable launcher asset is not a Windows executable')
    signature = (directory / (INSTALLER + '.sig')).read_text(encoding='utf-8').strip()
    update = json.loads((directory / 'launcher-update.json').read_text(encoding='utf-8'))
    if not isinstance(update, dict) or not isinstance(update.get('platforms'), dict):
        raise PublicationError('The immutable update feed is invalid')
    platform = update['platforms'].get('windows-x86_64', {})
    if not isinstance(platform, dict):
        raise PublicationError('The immutable Windows update platform is invalid')
    if (update.get('version') != version or set(update.get('platforms', {})) != {'windows-x86_64'}
            or platform.get('url') != expected[INSTALLER]['browser_download_url']
            or not signature or platform.get('signature') != signature):
        raise PublicationError('The immutable update feed does not match its installer and signature')
    checksum = (directory / 'LAUNCHER-SHA256SUMS.txt').read_text(encoding='ascii').strip()
    digest = expected[INSTALLER]['digest'][7:]
    if checksum != f'{digest}  {INSTALLER}':
        raise PublicationError('The immutable installer checksum file does not match GitHub and downloaded bytes')
    signature_check(installer, directory / (INSTALLER + '.sig'), version)
    return dict(version=version, url=platform['url'], sha256=digest,
                size=expected[INSTALLER]['size'], update=update)


def publish_metadata(root, metadata, *, attempts=5, run=command):
    """Retry only our single-file feed commit, retaining every concurrent main edit."""
    git = lambda *args, cwd=root, check=True: run(['git', *args], cwd=cwd, check=check)
    target_version = version_tuple(metadata['version'])
    for _ in range(attempts):
        git('fetch', '--quiet', 'origin', 'main')
        base = git('rev-parse', 'origin/main').stdout.strip()
        with tempfile.TemporaryDirectory(prefix='launcher-feed-') as temporary:
            checkout = Path(temporary) / 'site'
            git('worktree', 'add', '--quiet', '--detach', str(checkout), base)
            try:
                destination = checkout / METADATA
                if destination.is_symlink() or not destination.parent.resolve().is_relative_to(checkout.resolve()):
                    raise PublicationError('Website metadata path is unsafe')
                exists = destination.exists()
                current = json.loads(destination.read_text(encoding='utf-8')) if exists else None
                if exists:
                    if not isinstance(current, dict):
                        raise PublicationError('Existing website launcher metadata is invalid; it was preserved')
                    current_version = version_tuple(current.get('version'))
                    if current_version > target_version:
                        raise PublicationError('A newer launcher is already advertised; its feed was preserved')
                    if current_version == target_version:
                        if current != metadata:
                            raise PublicationError('This launcher version already has different website metadata; its feed was preserved')
                        return dict(metadata_commit=base, metadata_changed=False)
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(json.dumps(metadata, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
                git('add', '--', METADATA, cwd=checkout)
                changed = git('diff', '--cached', '--name-only', cwd=checkout).stdout.splitlines()
                if changed != [METADATA]:
                    raise PublicationError('Website publication must modify only launcher release metadata')
                git('-c', 'user.name=github-actions[bot]', '-c', 'user.email=41898282+github-actions[bot]@users.noreply.github.com',
                    'commit', '--quiet', '-m', f"Publish Windows launcher {metadata['version']} download and signed update feed", cwd=checkout)
                commit = git('rev-parse', 'HEAD', cwd=checkout).stdout.strip()
                if git('push', '--quiet', 'origin', 'HEAD:refs/heads/main', cwd=checkout, check=False).returncode == 0:
                    return dict(metadata_commit=commit, metadata_changed=True)
            finally:
                git('worktree', 'remove', '--force', str(checkout))
    raise PublicationError('Immutable launcher release is retained; website feed needs a same-source retry after concurrent main updates')


def publish(root, repository, source, bundle_directory, *, github=None, signature_verifier=verify_signature, attempts=5):
    root = Path(root).resolve()
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository) or not re.fullmatch(r'[0-9a-f]{40}', source):
        raise PublicationError('Publication requires an exact GitHub repository and full source commit')
    head = command(['git', 'rev-parse', 'HEAD'], cwd=root).stdout.strip()
    if head != source:
        raise PublicationError('The build checkout does not match the requested immutable source')
    origin = command(['git', 'remote', 'get-url', 'origin'], cwd=root).stdout.strip().removesuffix('.git')
    if origin not in (f'https://github.com/{repository}', f'git@github.com:{repository}'):
        raise PublicationError('The website origin is not the requested GitHub repository')
    package = json.loads(command(['git', 'show', source + ':launcher/package.json'], cwd=root).stdout)
    version = package['version']
    version_tuple(version)
    tag = 'launcher-v' + version
    github = github or GitHub(repository)
    release = github.release(tag)
    reused = release is not None
    with tempfile.TemporaryDirectory(prefix='launcher-release-') as temporary:
        temporary = Path(temporary)
        config = temporary / 'tauri.conf.json'
        config.write_text(command(['git', 'show', source + ':launcher/src-tauri/tauri.conf.json'], cwd=root).stdout, encoding='utf-8')
        verify = lambda installer, signature, version: signature_verifier(root, config, installer, signature, version)
        if release is None:
            existing_tag = github.tag_commit(tag, missing=True)
            if existing_tag is not None and existing_tag != source:
                raise PublicationError('This launcher tag already identifies a different source commit; bump the version')
            staging = temporary / 'new'
            staging.mkdir()
            bundles = list(Path(bundle_directory).glob('*.exe'))
            if len(bundles) != 1 or not Path(str(bundles[0]) + '.sig').is_file():
                raise PublicationError('A single signed Windows installer is required to create a release')
            shutil.copyfile(bundles[0], staging / INSTALLER)
            shutil.copyfile(str(bundles[0]) + '.sig', staging / (INSTALLER + '.sig'))
            verify(staging / INSTALLER, staging / (INSTALLER + '.sig'), version)
            signature = (staging / (INSTALLER + '.sig')).read_text(encoding='utf-8').strip()
            update = dict(version=version, notes=NOTES,
                          pub_date=datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
                          platforms={'windows-x86_64': dict(signature=signature,
                              url=f'https://github.com/{repository}/releases/download/{tag}/{INSTALLER}')})
            (staging / 'launcher-update.json').write_text(json.dumps(update, indent=2) + '\n', encoding='utf-8')
            (staging / 'LAUNCHER-SHA256SUMS.txt').write_text(f'{sha256(staging / INSTALLER)}  {INSTALLER}\n', encoding='ascii')
            # A create response can be lost after GitHub accepts it. Re-read and
            # independently verify the original immutable release in either case.
            github.create(tag, source, version, staging)
            release = github.release(tag)
            if release is None:
                raise PublicationError('Release creation was not confirmed; retry the same source without overwriting assets')
        downloaded = temporary / 'verified'
        downloaded.mkdir()
        metadata = verify_release(github, release, source, version, downloaded, verify)
    receipt = publish_metadata(root, metadata, attempts=attempts)
    return dict(version=version, source_commit=source, release=release['html_url'], reused_release=reused, **receipt)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--repository', required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--bundle-directory', type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        receipt = publish(args.root, args.repository, args.source, args.bundle_directory)
    except (PublicationError, OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f'Launcher publication incomplete: {error}\n')
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
