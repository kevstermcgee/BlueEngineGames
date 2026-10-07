"""Audit the generated public site, never the engine's optional web tooling."""
import argparse
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit


class DownloadPage(HTMLParser):
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        classes = attrs.get('class', '').split()
        if 'thumb' in classes and (tag != 'img' or 'icon' in classes or 'tile' in classes):
            raise ValueError('Game previews must use screenshots; icon and tile fallbacks are forbidden')
        if tag in {'iframe', 'embed', 'object', 'canvas'}:
            raise ValueError(f"Embedded game element: {tag}")
        for key in ('href', 'src'):
            path = unquote(urlsplit(attrs.get(key, '')).path).lower()
            if 'web' in path.split('/') or path.endswith(('.wasm', '.webmanifest', '.zip')):
                raise ValueError(f"Web-play or non-EXE download: {attrs[key]}")
        if tag == 'script' and Path(attrs.get('src', '')).name != 'app.js':
            raise ValueError('Only the catalog UI script is allowed')
        if tag == 'a' and 'dl' in attrs.get('class', '').split():
            if not urlsplit(attrs.get('href', '')).path.lower().endswith('.exe'):
                raise ValueError('Game downloads must link to Windows EXE assets')
        if 'data-browser' in attrs or 'play' in attrs.get('class', '').split():
            raise ValueError('Browser-play catalog entry')


def check(root):
    root = Path(root)
    if not (root / 'index.html').is_file():
        raise ValueError('Generated site is missing index.html')
    for file in root.rglob('*'):
        relative = file.relative_to(root)
        if 'web' in relative.parts or file.suffix.lower() in {'.wasm', '.webmanifest'} or file.name in {'service-worker.js', 'sw.js'}:
            raise ValueError(f"Web-play payload: {relative}")
        if file.suffix == '.html':
            try:
                DownloadPage().feed(file.read_text(encoding='utf-8'))
            except ValueError as error:
                raise ValueError(f"{relative}: {error}") from error


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('site', type=Path)
    args = parser.parse_args()
    check(args.site)
    print('Windows-only site audit passed: no web-play payloads or links; EXE downloads only.')
