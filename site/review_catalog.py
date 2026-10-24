"""Stop Pages publication when an advertised game cannot receive player reviews."""
from __future__ import annotations

import argparse
from http.client import HTTPException
import json
from pathlib import Path
import re
import socket
import sys
import time
from urllib.error import HTTPError, URLError
from urllib.request import HTTPRedirectHandler, Request, build_opener

ENDPOINT = "https://blue-engine.duckdns.org/community/v1/games"
MAX_BODY = 2_000_000
TIMEOUT = 15
RETRY_DELAYS = (5, 15)
TRANSIENT_STATUS = frozenset({408, 429, 500, 502, 503, 504})
SLUG = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*")


class CoverageError(ValueError):
    """Publication must wait for the receiver to cover the advertised games."""


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, request, file, code, message, headers, new_url):
        return None


def valid_slug(value):
    return isinstance(value, str) and len(value) <= 80 and SLUG.fullmatch(value)


def reviewable_games(document):
    if (not isinstance(document, dict) or type(document.get("schema_version")) is not int
            or document["schema_version"] != 1 or not isinstance(document.get("games"), list)):
        raise CoverageError("The review receiver returned an invalid game catalog. Restore the receiver before publishing.")
    games = document.get("reviewable_games")
    if not isinstance(games, dict):
        raise CoverageError("The review receiver does not report review eligibility. Deploy the current receiver before publishing.")
    if any(not valid_slug(slug) or not isinstance(title, str) or not title.strip() or len(title) > 160
           for slug, title in games.items()):
        raise CoverageError("The review receiver returned invalid review eligibility. Restore the receiver before publishing.")
    return games


def fetch_reviewable_games(opener):
    request = Request(ENDPOINT, headers={"Accept": "application/json", "Cache-Control": "no-cache"}, method="GET")
    with opener.open(request, timeout=TIMEOUT) as response:
        content = response.read(MAX_BODY + 1)
    if len(content) > MAX_BODY:
        raise CoverageError("The review receiver catalog exceeds the response limit. Restore the receiver before publishing.")
    try:
        document = json.loads(content)
    except (UnicodeError, ValueError, RecursionError):
        raise CoverageError("The review receiver returned malformed JSON. Restore the receiver before publishing.") from None
    return reviewable_games(document)


def check(document, *, opener=None, sleep=time.sleep):
    if (not isinstance(document, dict) or type(document.get("version")) is not int
            or document["version"] != 1 or not isinstance(document.get("games"), list)):
        raise CoverageError("The generated launcher catalog is invalid.")
    published = {}
    for game in document["games"]:
        if (not isinstance(game, dict) or not valid_slug(game.get("slug"))
                or not isinstance(game.get("name"), str) or not game["name"].strip()
                or game["slug"] in published):
            raise CoverageError("The generated launcher catalog has an invalid or duplicate game identity.")
        published[game["slug"]] = game["name"]
    opener = opener or build_opener(NoRedirect())
    for attempt in range(len(RETRY_DELAYS) + 1):
        try:
            accepted = fetch_reviewable_games(opener)
            missing = sorted(published.keys() - accepted.keys())
            if not missing:
                return {"published_games": len(published), "reviewable_games": len(accepted), "endpoint": ENDPOINT}
            names = ", ".join(f"{published[slug]} ({slug})" for slug in missing)
            failure = (f"Published games cannot receive reviews: {names}. Update the receiver's trusted game catalog "
                       "and rerun publication; the site was not deployed.")
        except HTTPError as error:
            if error.code not in TRANSIENT_STATUS:
                raise CoverageError(f"The review receiver returned HTTP {error.code}. Deploy or restore the receiver before publishing.") from None
            failure = f"The review receiver is unavailable after {attempt + 1} attempts (HTTP {error.code}). Restore it and rerun publication."
        except (TimeoutError, socket.timeout, URLError, OSError, HTTPException):
            failure = f"The review receiver is unavailable after {attempt + 1} attempts (a timeout or connection failure). Restore it and rerun publication."
        if attempt == len(RETRY_DELAYS):
            raise CoverageError(failure)
        # Receiver reads cache trusted Git snapshots briefly. The same bounded
        # budget covers network recovery and a just-published game's propagation.
        sleep(RETRY_DELAYS[attempt])


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("catalog", type=Path, help="Generated _site/catalog.json")
    args = parser.parse_args(argv)
    try:
        result = check(json.loads(args.catalog.read_text(encoding="utf-8")))
        print(f"Player review coverage verified: all {result['published_games']} published games are reviewable.")
        return 0
    except (CoverageError, OSError, ValueError) as error:
        print(f"Player review coverage: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
