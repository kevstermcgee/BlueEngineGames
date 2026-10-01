#!/usr/bin/env python3
"""Generate the static download site for BlueEngineGames.

Joins the release catalog TSV (source of truth for which games exist) with the
GitHub release JSON (per-asset sizes) and the repo checkout (thumbnails, icons,
git dates), and emits a self-contained _site/ directory for GitHub Pages.

Usage:
  python3 site/build.py --catalog catalog.tsv --release-json release.json \
      --games-dir games --thumbs site/thumbs --out _site
"""

import argparse
import csv
import html
import json
import shutil
import subprocess
import sys
from pathlib import Path

REPO = "kevstermcgee/BlueEngineGames"
LATEST = f"https://github.com/{REPO}/releases/latest/download"

PAGE = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>BlueEngine Games</title>
<link rel="stylesheet" href="style.css">
</head>
<body>
<header>
  <h1>BlueEngine Games</h1>
  <p class="tagline">Free games for Windows. Download a zip, unzip it, run the exe.</p>
</header>

<section class="notes">
  <ul>
    <li>Windows x64 only.</li>
    <li>The exes aren't code-signed, so SmartScreen may warn you: click <strong>More info &rarr; Run anyway</strong>.</li>
    <li>Verify downloads against <a href="{sums_url}">SHA256SUMS.txt</a>.</li>
    <li>Prefer an app? Get the <a href="{launcher_url}">BlueEngine Launcher</a> &mdash; it lists and installs everything below.</li>
  </ul>
</section>

<div class="toolbar">
  <input id="search" type="search" placeholder="Search games&hellip;" aria-label="Search games">
  <select id="sort" aria-label="Sort games">
    <option value="newest" selected>Newest first</option>
    <option value="oldest">Oldest first</option>
    <option value="name">Name A&ndash;Z</option>
    <option value="size">Smallest first</option>
  </select>
  <div class="kinds" role="group" aria-label="Filter by kind">
    <button data-kind="" class="active">All</button>
    <button data-kind="native">Native</button>
    <button data-kind="data">Data games</button>
  </div>
</div>

<main id="grid">
{cards}
</main>
<p id="empty" hidden>No games match.</p>

<footer>
  <p>Release <a href="{release_url}">{release_name}</a> &middot; published {release_date} &middot;
     <a href="https://github.com/{repo}">source on GitHub</a></p>
</footer>
<script src="app.js"></script>
</body>
</html>
"""

CARD = """<article class="card" data-name="{name_attr}" data-created="{created}" data-kind="{kind_attr}" data-size="{bytes}" data-text="{text_attr}">
  {thumb}
  <div class="body">
    <h2>{name}{badge}</h2>
    <p class="desc">{desc}</p>
    <p class="meta">{version}added {created}{size}</p>
    <a class="dl" href="{asset}">Download</a>
  </div>
</article>"""


def game_added_date(repo_root: Path, slug: str) -> str:
    """First commit date that touched games/<slug>, or '' if unknown."""
    try:
        out = subprocess.run(
            ["git", "log", "--reverse", "--format=%as", "--", f"games/{slug}"],
            cwd=repo_root, capture_output=True, text=True, timeout=60,
        ).stdout
        return out.splitlines()[0].strip() if out.strip() else ""
    except Exception:
        return ""


def human_size(n: int) -> str:
    return f"{n / 1_000_000:.1f} MB"


def version_label(v: str) -> str:
    """'0.1.0' -> 'v0.1.0 · '; placeholder values like 'catalog' are dropped."""
    return f"v{html.escape(v)} &middot; " if v and v[0].isdigit() else ""


def build(args) -> None:
    repo_root = Path(args.games_dir).resolve().parent
    out = Path(args.out)
    thumbs_out = out / "thumbs"
    thumbs_out.mkdir(parents=True, exist_ok=True)

    with open(args.release_json) as f:
        release = json.load(f)
    asset_sizes = {a["name"]: a["size"] for a in release.get("assets", [])}

    with open(args.catalog, newline="") as f:
        rows = list(csv.DictReader(f, delimiter="\t"))

    games = []
    for row in rows:
        slug = row["slug"]
        created = game_added_date(repo_root, slug) or row.get("created", "")
        zip_name = f"{slug}-windows-x64.zip"
        size = asset_sizes.get(zip_name)
        if size is None:
            print(f"warning: no release asset for {slug}", file=sys.stderr)

        thumb_src = Path(args.thumbs) / f"{slug}.png"
        icon_src = Path(args.games_dir) / slug / "assets" / "icon.png"
        if thumb_src.is_file():
            shutil.copy2(thumb_src, thumbs_out / f"{slug}.png")
            thumb = f'<img class="thumb" src="thumbs/{slug}.png" alt="" width="320" height="180" loading="lazy">'
        elif icon_src.is_file():
            shutil.copy2(icon_src, thumbs_out / f"{slug}.png")
            thumb = f'<img class="thumb icon" src="thumbs/{slug}.png" alt="" width="320" height="180" loading="lazy">'
        else:
            initials = html.escape(row["name"][:2])
            thumb = f'<div class="thumb tile">{initials}</div>'

        kind = "data" if "data" in row.get("kind", "") else "native"
        name = html.escape(row["name"])
        games.append({
            "card": CARD.format(
                name=name,
                name_attr=name.lower(),
                text_attr=html.escape((row["name"] + " " + row.get("description", "")).lower()),
                desc=html.escape(row.get("description", "")),
                created=html.escape(created),
                kind_attr=kind,
                badge=' <span class="badge">data game</span>' if kind == "data" else "",
                version=version_label(row.get("game_version", "")),
                size=f" &middot; {human_size(size)}" if size is not None else "",
                bytes=size if size is not None else 0,
                asset=html.escape(row["asset"]),
                thumb=thumb,
            ),
            "created": created,
            "name": row["name"],
        })

    games.sort(key=lambda g: g["name"])
    games.sort(key=lambda g: g["created"], reverse=True)  # newest first, A-Z within a day

    page = PAGE.format(
        cards="\n".join(g["card"] for g in games),
        sums_url=f"{LATEST}/SHA256SUMS.txt",
        launcher_url=f"{LATEST}/BlueEngineLauncher-windows-x64.zip",
        release_url=html.escape(release.get("html_url", f"https://github.com/{REPO}/releases/latest")),
        release_name=html.escape(release.get("name") or release.get("tag_name", "latest")),
        release_date=html.escape((release.get("published_at") or "")[:10]),
        repo=REPO,
    )

    (out / "index.html").write_text(page)
    site_dir = Path(__file__).resolve().parent
    shutil.copy2(site_dir / "style.css", out / "style.css")
    shutil.copy2(site_dir / "app.js", out / "app.js")
    (out / ".nojekyll").touch()
    print(f"wrote {out}/index.html with {len(games)} games")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--catalog", required=True)
    p.add_argument("--release-json", required=True)
    p.add_argument("--games-dir", default="games")
    p.add_argument("--thumbs", default="site/thumbs")
    p.add_argument("--out", default="_site")
    build(p.parse_args())
