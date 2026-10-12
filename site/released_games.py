"""Read and verify native installers published in independent game release repositories."""
import argparse
import hashlib
import json
import re
import urllib.request
from pathlib import Path


def download_for(release, slug):
    filename = slug + '-setup-windows-x64.exe'
    for asset in release['assets']:
        if asset['name'] == filename:
            return asset['browser_download_url'], 'Download installer (.exe)', asset['size']
    return '', '', 0


def load(repo_root):
    """Catalog verified installers published in a game's own release repository."""
    path = repo_root / 'site/released-games.json'
    if not path.is_file():
        return []
    document = json.loads(path.read_text(encoding='utf-8'))
    if document.get('version') != 1 or not isinstance(document.get('games'), list):
        raise ValueError('Invalid independently released game catalog')
    seen = set()
    for game in document['games']:
        slug = game['slug']
        if not re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*', slug) or slug in seen:
            raise ValueError('Unsafe or duplicate independently released game slug')
        seen.add(slug)
        release = game['published_release']
        if release.get('draft') or release.get('prerelease') or not release.get('published_at'):
            raise ValueError(f'{slug} needs a published release')
        asset, _, size = download_for(release, slug)
        if not re.fullmatch(r'https://github\.com/[\w.-]+/[\w.-]+/releases/download/[^/]+/'
                            + re.escape(slug) + r'-setup-windows-x64\.exe', asset):
            raise ValueError(f'{slug} needs a pinned Windows installer URL')
        if '/latest/' in asset or asset.split('/')[-2] != release['tag_name'] or size <= 0:
            raise ValueError(f'{slug} needs an immutable release and installer size')
        metadata = next(a for a in release['assets'] if a['browser_download_url'] == asset)
        if not re.fullmatch(r'sha256:[0-9a-f]{64}', metadata.get('digest', '')):
            raise ValueError(f'{slug} needs a SHA256-pinned installer')
        if game.get('presentation') not in ('2d', '3d', 'hybrid') or game.get('networking') not in ('offline', 'native-multiplayer'):
            raise ValueError(f'{slug} needs presentation and networking metadata')
        if not re.fullmatch(r'https://github\.com/[\w.-]+/[\w.-]+/tree/[0-9a-f]{40}/.+', game['source_url']):
            raise ValueError(f'{slug} needs pinned public source')
    return document['games']


def verify(root):
    for game in load(root):
        asset = next(a for a in game['published_release']['assets']
                     if a['name'] == game['slug'] + '-setup-windows-x64.exe')
        with urllib.request.urlopen(asset['browser_download_url'], timeout=60) as response:
            data = response.read()
        if not data.startswith(b'MZ') or len(data) != asset['size']:
            raise ValueError(f"{game['slug']}: installer bytes or size differ from the release")
        if 'sha256:' + hashlib.sha256(data).hexdigest() != asset['digest']:
            raise ValueError(f"{game['slug']}: published installer SHA256 differs from the catalog")
        print(f"Verified {game['slug']} Windows installer ({len(data)} bytes).")


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    verify(parser.parse_args().root)
