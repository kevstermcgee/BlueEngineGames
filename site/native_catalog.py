"""Add native library filters and favorites without introducing other products."""
import html
import json
import re
import shutil
from pathlib import Path


def esc(value):
    return html.escape(str(value), quote=True)


def star(slug, title):
    return f'<button class="star" data-star="{esc(slug)}" aria-pressed="false" aria-label="Star {esc(title)}"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m12 2.5 2.94 5.96 6.58.96-4.76 4.64 1.12 6.55L12 17.52l-5.88 3.09 1.12-6.55L2.48 9.42l6.58-.96Z"/></svg></button>'


def enrich_games(games, rows, out, games_dir):
    shutil.copy2(Path(__file__).with_name('catalog.css'), out / 'catalog.css')
    result = []
    for row, game in zip(rows, games):
        slug = row['slug']
        project_path = Path(games_dir) / slug / 'game.project.json'
        project = json.loads(project_path.read_text(encoding='utf-8')) if project_path.is_file() else {}
        presentation = project.get('presentation', '3d')
        network = project.get('networking', 'native-multiplayer' if row.get('online_args_b64') else 'unknown')
        attributes = f' data-id="{esc(slug)}" data-presentation="{esc(presentation)}" data-networking="{esc(network)}"'
        body = game['card'].replace('<article ', '<article' + attributes + ' ', 1)
        body = re.sub(r'<h2>(.*?)</h2>', lambda m: f'<div class="card-heading"><h2>{m.group(1)}</h2>{star(slug, row["name"])}</div>', body, count=1, flags=re.S)
        body = body.replace('<p class="meta">', f'<p class="meta">{esc(presentation.upper())} · ', 1)
        result.append(game | {'card': body})
    return result


def enhance_page(page):
    page = page.replace('</head>', '<link rel="stylesheet" href="catalog.css"></head>', 1)
    page = page.replace('</p>\n</header>', ' Star favorites to keep them first on this device.</p>\n</header>', 1)
    filters = '''<select id="presentation" aria-label="Presentation"><option value="">2D + 3D + Hybrid</option><option value="2d">2D</option><option value="3d">3D</option><option value="hybrid">Hybrid</option></select>
<select id="networking" aria-label="Networking"><option value="">All networking</option><option value="offline">Singleplayer</option><option value="native-multiplayer">Multiplayer</option></select>'''
    page = page.replace('<div class="kinds"', filters + '<div class="kinds"', 1)
    return page.replace('<main id="grid">', '<p id="favorites-status" role="status"></p><main id="grid">', 1)
