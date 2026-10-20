"""Pinned package feed shared by the native Windows launcher and static site."""
import copy
import datetime
import hashlib
import json
import re

BASE = 'https://kevstermcgee.github.io/BlueEngineGames/'


def released_at(release):
    """Use the published timestamp only; an absent date remains explicitly unknown."""
    value = release.get('published_at')
    if not isinstance(value, str):
        return None
    try:
        parsed = datetime.datetime.fromisoformat(value.replace('Z', '+00:00'))
    except ValueError:
        return None
    return value if parsed.tzinfo is not None else None


def historical_package(release, slug, independent):
    """Offer an older package only when its public asset has complete pinned metadata."""
    kinds = [('installer', '-setup-windows-x64.exe'), ('zip', '-windows-x64.zip')]
    if not independent:
        kinds.reverse()
    for kind, suffix in kinds:
        asset = next((a for a in release.get('assets', []) if a.get('name') == slug + suffix), None)
        if not asset:
            continue
        digest = asset.get('digest') or ''
        size, url = asset.get('size'), asset.get('browser_download_url')
        if (not re.fullmatch(r'sha256:[0-9a-fA-F]{64}', digest)
                or not isinstance(size, int) or isinstance(size, bool) or not 0 < size <= 8 * 1024**3
                or not isinstance(url, str) or not url):
            continue
        return dict(kind=kind, url=url, size=size, sha256=digest[7:].lower(), release=release['tag_name'])
    return None


def available_versions(row, current, package, releases):
    slug = row['slug']
    independent = 'published_release' in row
    history = row.get('previous_releases', releases)
    filenames = {slug + '-setup-windows-x64.exe', slug + '-windows-x64.zip'}
    seen = {current['tag_name']}
    older = []
    for release in history:
        tag = release.get('tag_name')
        if (not tag or tag in seen or release.get('draft') or release.get('prerelease')
                or not any(a.get('name') in filenames for a in release.get('assets', []))):
            continue
        seen.add(tag)
        older.append(dict(release=tag, name=release.get('name') or tag,
                          released_at=released_at(release), latest=False,
                          package=historical_package(release, slug, independent)))
    # Unknown dates follow dated releases; never substitute the game's creation date.
    older.sort(key=lambda version: version['released_at'] or '', reverse=True)
    return [dict(release=current['tag_name'], name=current.get('name') or current['tag_name'],
                 released_at=released_at(current), latest=True, package=copy.deepcopy(package)), *older]


def entry(row, release, wiki, image_root, origin, created, releases=()):
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
    # Released launchers through 0.1.11 iterate this list unconditionally. Keep
    # their feed shape compatible without inventing story or character facts.
    doc.setdefault('characters', [])
    for picture in [*doc['gallery'], *doc['characters']]:
        if picture.get('image'):
            digest_image = hashlib.sha256((image_root / picture['image']).read_bytes()).hexdigest()[:20]
            picture['image'] = f'{BASE}games/{slug}/images/{digest_image}.png'
    package = dict(kind='installer' if independent else 'zip',
                   url=asset.get('browser_download_url') or row['asset'],
                   size=asset['size'], sha256=digest, release=release['tag_name'])
    return dict(slug=slug, name=row['name'], description=row.get('description', ''),
                version=row.get('game_version', ''), created=created, origin=origin,
                released_at=released_at(release), versions=available_versions(row, release, package, releases),
                presentation=row.get('presentation', ''), networking=row.get('networking', ''),
                image=f'{BASE}thumbs/{slug}.png', guide_url=f'{BASE}games/{slug}/', wiki=doc,
                package=package)


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
