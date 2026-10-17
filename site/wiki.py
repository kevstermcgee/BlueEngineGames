"""Validate and render public game guides. New releases require complete evidence.

The fixed legacy roster relaxes only unrecoverable provenance and missing images;
it never exempts a new slug or permits invented prompts, lore, or screenshots.
"""
import argparse
import hashlib
import html
import json
import re
import shutil
import struct
from pathlib import Path, PurePosixPath
from urllib.parse import quote


def text(value, label, minimum=1):
    if not isinstance(value, str) or len(value.strip()) < minimum:
        raise ValueError(f'Wiki {label} needs at least {minimum} characters')
    return value.strip()


def image_file(root, value, *, character=False):
    name = text(value, 'image path')
    relative = PurePosixPath(name)
    if (relative.is_absolute() or '\\' in name or '..' in relative.parts
            or relative.suffix.lower() != '.png'):
        raise ValueError(f'Unsafe wiki image path: {name}')
    path = (Path(root) / name).resolve()
    if not path.is_relative_to(Path(root).resolve()) or not path.is_file():
        raise ValueError(f'Missing or unsafe wiki image: {name}')
    data = path.read_bytes()
    if len(data) < 33 or data[:8] != b'\x89PNG\r\n\x1a\n' or data[12:16] != b'IHDR':
        raise ValueError(f'Wiki image must be a PNG: {name}')
    width, height = struct.unpack('>II', data[16:24])
    if ((character and min(width, height) < 96)
            or (not character and (width < 320 or height < 180 or width <= height))):
        raise ValueError(f'Wiki screenshot has insufficient dimensions: {name}')
    return path


def validate_wiki(root, document, origin, legacy=False):
    if not isinstance(document, dict) or document.get('version') != 1:
        raise ValueError('Wiki document needs version 1')
    text(document.get('overview'), 'overview', 120)
    text(document.get('story'), 'story or explicit absence of a story', 40)
    for field, count, minimum in (('rules', 3, 20), ('controls', 1, 8)):
        items = document.get(field)
        if not isinstance(items, list) or len(items) < count:
            raise ValueError(f'Wiki needs at least {count} {field}')
        for item in items:
            text(item, field, minimum)
    characters = document.get('characters')
    if not isinstance(characters, list):
        raise ValueError('Wiki characters must be a list')
    if not characters:
        text(document.get('characters_note'), 'explanation of no characters', 20)
    names = set()
    for character in characters:
        if not isinstance(character, dict):
            raise ValueError('Invalid wiki character')
        name = text(character.get('name'), 'character name')
        if name in names:
            raise ValueError(f'Duplicate wiki character: {name}')
        names.add(name)
        text(character.get('description'), 'character description', 40)
        if character.get('image'):
            image_file(root, character['image'], character=True)
        elif not legacy:
            raise ValueError(f'Wiki character needs an in-game screenshot: {name}')
    gallery = document.get('gallery')
    if not isinstance(gallery, list) or len(gallery) < (1 if legacy else 3):
        raise ValueError('Wiki needs at least three distinct gameplay screenshots (one for legacy games)')
    hashes = set()
    for item in gallery:
        if not isinstance(item, dict):
            raise ValueError('Invalid wiki gallery entry')
        text(item.get('caption'), 'image caption', 8)
        text(item.get('alt'), 'image alternative text', 8)
        path = image_file(root, item.get('image'))
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if digest in hashes:
            raise ValueError('Wiki gallery screenshots must be distinct, not duplicated files')
        hashes.add(digest)
    design = document.get('design')
    if not isinstance(design, dict) or design.get('origin') != origin:
        raise ValueError('Wiki design credit does not match the catalog')
    if origin == 'ai':
        if not legacy or design.get('rationale'):
            text(design.get('rationale'), 'public AI design rationale', 120)
        if not legacy and ('original_prompt' not in design or design['original_prompt'] is not None):
            raise ValueError('AI wiki original_prompt must be null')
    elif origin == 'human':
        if not legacy or design.get('original_prompt'):
            text(design.get('original_prompt'), 'original human creation prompt')
        elif design.get('prompt_status') != 'unavailable':
            raise ValueError('Legacy wiki must explicitly mark an unrecovered original prompt')
    else:
        raise ValueError('Invalid wiki design origin')
    return document


def legacy_slugs(repo_root):
    path = Path(repo_root) / 'site/wiki/legacy-games.json'
    if not path.is_file():
        return set()
    document = json.loads(path.read_text(encoding='utf-8'))
    slugs = document.get('slugs')
    if document.get('version') != 1 or not isinstance(slugs, list) or any(
            not isinstance(slug, str) or not re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*', slug) for slug in slugs):
        raise ValueError('Invalid fixed legacy wiki roster')
    return set(slugs)


def load_wiki(repo_root, games_dir, row, origin):
    slug = row['slug']
    if not re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*', slug):
        raise ValueError('Unsafe wiki game slug')
    paths = [Path(repo_root) / 'site/wiki' / slug / 'wiki.json', Path(games_dir) / slug / 'wiki.json']
    path = next((path for path in paths if path.is_file()), None)
    if path is None:
        raise ValueError(f'{slug}: add a complete wiki.json and screenshot gallery before publishing')
    if not path.resolve().is_relative_to(Path(repo_root).resolve()):
        raise ValueError(f'{slug}: wiki document escapes the source repository')
    document = json.loads(path.read_text(encoding='utf-8'))
    legacy = slug in legacy_slugs(repo_root)
    try:
        validate_wiki(path.parent, document, origin, legacy)
    except ValueError as error:
        raise ValueError(f'{slug}: {error}') from error
    return document, path.parent, legacy


def render_wiki(document, image_root, destination, legacy=False):
    """Render escaped plain text and copy only validated local images."""
    esc = html.escape
    image_out = Path(destination) / 'images'
    image_out.mkdir(exist_ok=True)
    def image_url(name):
        source = image_file(image_root, name, character=True)
        filename = hashlib.sha256(source.read_bytes()).hexdigest()[:20] + '.png'
        target = image_out / filename
        if not target.exists():
            shutil.copy2(source, target)
        return 'images/' + filename
    def paragraphs(value):
        return ''.join(f'<p>{esc(part)}</p>' for part in value.split('\n\n') if part.strip())
    def listing(items, ordered=False):
        tag = 'ol' if ordered else 'ul'
        return f'<{tag}>' + ''.join(f'<li>{esc(item)}</li>' for item in items) + f'</{tag}>'
    def figure(item):
        url = image_url(item['image'])
        return (f'<figure><a href="{url}"><img src="{url}" alt="{esc(item["alt"], quote=True)}" '
                f'loading="lazy" width="640" height="360"></a><figcaption>{esc(item["caption"])}</figcaption></figure>')
    characters = ''
    for character in document['characters']:
        picture = ''
        if character.get('image'):
            url = image_url(character['image'])
            picture = f'<a href="{url}"><img src="{url}" alt="{esc(character["name"], quote=True)} in-game screenshot" loading="lazy"></a>'
            if character.get('image_caption'):
                picture += f'<p class="wiki-source-note">{esc(character["image_caption"])}</p>'
        else:
            picture = ('<p class="archive-note">No separate character model is rendered in this release.</p>'
                       if character.get('image_status') == 'not-rendered' else
                       '<p class="archive-note">Character screenshot not yet archived.</p>')
        appearance = paragraphs(character['appearance_note']) if character.get('appearance_note') else ''
        characters += f'<article class="wiki-character">{picture}<h3>{esc(character["name"])}</h3>{paragraphs(character["description"])}{appearance}</article>'
    if not characters:
        characters = paragraphs(document['characters_note'])
    design = document['design']
    if design['origin'] == 'ai':
        title = 'Why the AI chose this game'
        provenance = paragraphs(design['rationale']) if design.get('rationale') else '<p class="archive-note">Original AI design rationale not yet recovered.</p>'
        if design.get('rationale_source'):
            provenance += f'<p class="wiki-source-note">{esc(design["rationale_source"])}</p>'
    else:
        title = 'Original human creation prompt'
        provenance = (f'<blockquote class="creation-prompt">{esc(design["original_prompt"])}</blockquote>'
                      if design.get('original_prompt') else '<p class="archive-note">Original prompt not yet recovered.</p>')
        if design.get('prompt_note'):
            provenance += f'<p class="wiki-source-note">{esc(design["prompt_note"])}</p>'
    gallery = ''.join(figure(item) for item in document['gallery'])
    sources = []
    for source in document.get('sources', []):
        if isinstance(source, str) and not PurePosixPath(source).is_absolute() and '..' not in PurePosixPath(source).parts:
            url = 'https://github.com/kevstermcgee/BlueEngineGames/blob/main/' + quote(source, safe='/')
            sources.append(f'<li><a href="{esc(url, quote=True)}">{esc(source)}</a></li>')
    documentation = '<details class="wiki-sources"><summary>Source documentation</summary><ul>' + ''.join(sources) + '</ul></details>' if sources else ''
    note = ('<p class="archive-note">More gameplay and character captures are being archived for this earlier release.</p>'
            if legacy and (len(document['gallery']) < 3 or any(not c.get('image') for c in document['characters'])) else '')
    return f'''<section class="game-wiki" aria-labelledby="guide-heading">
  <h2 id="guide-heading">Game guide</h2>
  <nav class="wiki-nav" aria-label="Game guide sections"><a href="#overview">Overview</a><a href="#rules">Rules</a><a href="#controls">Controls</a><a href="#story">Story</a><a href="#characters">Characters</a><a href="#gallery">Screenshots</a><a href="#creation">Creation</a><a href="#versions-heading">Versions</a></nav>
  <section id="overview"><h2>What is this game?</h2>{paragraphs(document['overview'])}</section>
  <section id="rules"><h2>Rules &amp; objectives</h2>{listing(document['rules'], True)}</section>
  <section id="controls"><h2>How to play</h2>{listing(document['controls'])}</section>
  <section id="story"><h2>Story &amp; setting</h2>{paragraphs(document['story'])}</section>
  <section id="characters"><h2>Characters &amp; roles</h2><div class="wiki-characters">{characters}</div></section>
  <section id="gallery"><h2>Screenshot gallery</h2><p>Open an image to see the full capture.</p><div class="wiki-gallery">{gallery}</div>{note}</section>
  <section id="creation"><h2>{title}</h2>{provenance}</section>
  {documentation}
</section>'''


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--game-dir', type=Path, required=True)
    parser.add_argument('--origin', choices=('ai', 'human'), required=True)
    args = parser.parse_args()
    validate_wiki(args.game_dir, json.loads((args.game_dir / 'wiki.json').read_text(encoding='utf-8')), args.origin)
    print('Complete game wiki and screenshot gallery verified.')
