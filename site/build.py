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
  <p class="tagline">Free games for Windows. Download an installer, choose a desktop shortcut, and play.</p>
</header>

<section class="notes">
  <ul>
    <li>Windows x64 only.</li>
    <li>Verify downloads against <a href="{sums_url}">SHA256SUMS.txt</a>.</li>
    <li>Installed games have a <strong>Check for updates</strong> shortcut in the Start Menu.</li>
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

CARD = """<article class="card" id="{slug}" data-slug="{slug}" data-name="{name_attr}" data-created="{created}" data-kind="{kind_attr}" data-size="{bytes}" data-text="{text_attr}">
  <a class="preview-link" href="games/{slug}/">{thumb}</a>
  <div class="body">
    <h2><a class="game-title" href="games/{slug}/">{name}</a>{badge}</h2>
    <p class="desc">{desc}</p>
    <p class="meta">{version}added {created}{size}</p>
    <a class="dl" href="{asset}">{download_label}</a>
    <a class="all-versions" href="games/{slug}/">Game details &amp; versions</a>
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


def release_download(release: dict, filename: str) -> str:
    """Only offer artifacts that actually exist in this immutable release."""
    for asset in release.get("assets", []):
        if asset["name"] == filename:
            return asset.get("browser_download_url") or f"https://github.com/{REPO}/releases/download/{release['tag_name']}/{filename}"
    return ""


def download_for(release: dict, slug: str) -> tuple[str, str, int]:
    filename = slug + "-setup-windows-x64.exe"
    url = release_download(release, filename)
    if url:
        size = next(a.get("size", 0) for a in release["assets"] if a["name"] == filename)
        return url, "Download installer (.exe)", size
    return "", "Installer not yet available", 0


def version_rows(releases: list[dict], slug: str, current_tag: str) -> str:
    items = []
    for release in releases:
        if release.get("draft") or release.get("prerelease"):
            continue
        url, label, size = download_for(release, slug)
        if not url and not release_download(release, slug + "-windows-x64.zip"):
            continue
        esc = html.escape
        date = (release.get("published_at") or "")[:10]
        tag = release["tag_name"]
        latest = ' <span class="badge">Latest</span>' if tag == current_tag else ''
        sums = release_download(release, "INSTALLER-SHA256SUMS.txt") or release_download(release, "SHA256SUMS.txt")
        checksum = f'<a href="{esc(sums)}">Checksums</a>' if sums and url else ""
        download = f'<a href="{esc(url)}">{label}</a><br><span class="version-size">{human_size(size)}</span>' if url else '<span class="version-size">Installer not yet available</span>'
        items.append(f'<tr><td>{esc(date)}{latest}</td><td><code>{esc(tag)}</code></td><td>{download}</td><td>{checksum}</td></tr>')
    return "\n".join(items)


GAME_PAGE = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{name} · BlueEngine Games</title>
<link rel="stylesheet" href="../../style.css">
</head>
<body>
<header>
  <a href="../../">&larr; All games</a>
  <h1>{name}</h1>
</header>
<main>
  <section class="game-summary">
    {thumb}
    <div>
      <p>{desc}</p>
      <p class="meta">{version}added {created} &middot; Windows x64</p>
      <a class="dl" href="{asset}">{download_label}</a>
      <p class="game-help">Install once, then use the game's <strong>Check for updates</strong> Start Menu shortcut. Updates keep your saves and settings.</p>
    </div>
  </section>
  <section class="version-history" aria-labelledby="versions-heading">
    <h2 id="versions-heading">Versions</h2>
    <p>The latest release is the default. Choose an older release below to revisit a previous build.</p>
    <table>
      <thead><tr><th>Released</th><th>Release</th><th>Download</th><th>Verify</th></tr></thead>
      <tbody>{versions}</tbody>
    </table>
  </section>
</main>
<footer><a href="https://github.com/{repo}">Source on GitHub</a></footer>
</body>
</html>
"""



def build(args) -> None:
    repo_root = Path(args.games_dir).resolve().parent
    out = Path(args.out)
    thumbs_out = out / "thumbs"
    thumbs_out.mkdir(parents=True, exist_ok=True)

    with open(args.release_json) as f:
        release = json.load(f)
    releases = []
    history_path = getattr(args, "releases_json", None)
    if history_path:
        with open(history_path) as f:
            releases = json.load(f)
    releases.sort(key=lambda r: r.get("published_at") or "", reverse=True)

    with open(args.catalog, newline="") as f:
        rows = list(csv.DictReader(f, delimiter="\t"))

    games = []
    for row in rows:
        slug = row["slug"]
        created = game_added_date(repo_root, slug) or row.get("created", "")
        asset, download_label, size = download_for(release, slug)
        if not asset:
            raise ValueError(f"Catalog game has no installer in the published release: {slug}")

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
                slug=html.escape(slug),
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
                asset=html.escape(asset),
                download_label=download_label,
                thumb=thumb,
            ),
            "created": created,
            "name": row["name"],
        })

        versions = [release] + [r for r in releases if r.get("tag_name") != release.get("tag_name")]
        game_out = out / "games" / slug
        game_out.mkdir(parents=True, exist_ok=True)
        (game_out / "index.html").write_text(GAME_PAGE.format(
            name=name, desc=html.escape(row.get("description", "")),
            thumb=thumb.replace('src="thumbs/', 'src="../../thumbs/'),
            version=version_label(row.get("game_version", "")), created=html.escape(created),
            asset=html.escape(asset), download_label=download_label,
            versions=version_rows(versions, slug, release.get("tag_name", "")), repo=REPO,
        ))

    games.sort(key=lambda g: g["name"])
    games.sort(key=lambda g: g["created"], reverse=True)  # newest first, A-Z within a day

    page = PAGE.format(
        cards="\n".join(g["card"] for g in games),
        sums_url=html.escape(release_download(release, "INSTALLER-SHA256SUMS.txt") or release_download(release, "SHA256SUMS.txt") or f"{LATEST}/SHA256SUMS.txt"),
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
    p.add_argument("--releases-json", help="All published releases, including their asset metadata")
    p.add_argument("--games-dir", default="games")
    p.add_argument("--thumbs", default="site/thumbs")
    p.add_argument("--out", default="_site")
    build(p.parse_args())
