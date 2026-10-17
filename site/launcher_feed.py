"""Pinned package feed shared by the native Windows launcher and static site."""
import copy
import hashlib
import json

BASE = 'https://kevstermcgee.github.io/BlueEngineGames/'


def entry(row, release, wiki, image_root, origin, created):
    slug = row['slug']
    independent = 'published_release' in row
    filename = slug + ('-setup-windows-x64.exe' if independent else '-windows-x64.zip')
    asset = next((a for a in release['assets'] if a['name'] == filename), None)
    if not asset:
        raise ValueError(f'{slug}: missing launcher package')
    digest = asset.get('digest', '').removeprefix('sha256:') if independent else row.get('sha256', '')
    if len(digest) != 64 or any(c not in '0123456789abcdefABCDEF' for c in digest):
        raise ValueError(f'{slug}: launcher package needs a SHA256')
    doc = copy.deepcopy(wiki)
    for picture in [*doc['gallery'], *doc['characters']]:
        if picture.get('image'):
            digest_image = hashlib.sha256((image_root / picture['image']).read_bytes()).hexdigest()[:20]
            picture['image'] = f'{BASE}games/{slug}/images/{digest_image}.png'
    return dict(slug=slug, name=row['name'], description=row.get('description', ''),
                version=row.get('game_version', ''), created=created, origin=origin,
                presentation=row.get('presentation', ''), networking=row.get('networking', ''),
                image=f'{BASE}thumbs/{slug}.png', guide_url=f'{BASE}games/{slug}/', wiki=doc,
                package=dict(kind='installer' if independent else 'zip',
                             url=asset.get('browser_download_url') or row['asset'],
                             size=asset['size'], sha256=digest, release=release['tag_name']))


def write(out, entries):
    (out / 'catalog.json').write_text(json.dumps(dict(version=1, games=entries), ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def publish_launcher(repo_root, out, page):
    path = repo_root / 'site/launcher-release.json'
    if not path.is_file():
        return page
    metadata = json.loads(path.read_text(encoding='utf-8'))
    from urllib.parse import urlsplit
    import html
    url = metadata['url']
    parsed = urlsplit(url)
    if parsed.scheme != 'https' or parsed.netloc != 'github.com' or not parsed.path.startswith('/kevstermcgee/BlueEngineGames/releases/download/launcher-v') or not parsed.path.endswith('.exe'):
        raise ValueError('Untrusted launcher download URL')
    if metadata['update']['platforms']['windows-x86_64']['url'] != url or not metadata['update']['platforms']['windows-x86_64']['signature']:
        raise ValueError('Launcher needs a signed app update')
    (out / 'launcher-update.json').write_text(json.dumps(metadata['update'], indent=2) + '\n', encoding='utf-8')
    (out / 'launcher-release.json').write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8')
    banner = f'''<section class="launcher-banner"><div><p class="launcher-kicker">YOUR GAMES. ONE HOME.</p><h2>Meet BlueEngine Games for Windows</h2><p>Install, play and update your games from one library. Keep your favorites, explore game guides and take your installed games offline.</p></div><div><a class="dl" href="{html.escape(url, quote=True)}">Download the Windows launcher</a><p class="launcher-meta">Windows 10/11 · x64 · v{html.escape(metadata['version'])} · {metadata['size'] / 1000000:.1f} MB</p><p class="launcher-meta"><a href="launcher-release.json">Download details &amp; SHA256</a></p></div></section>'''
    return page.replace('<section class="notes">', banner + '\n<section class="notes">', 1)
