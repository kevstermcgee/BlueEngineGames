#!/usr/bin/env bash
# Build deadfall-hub and deadfall-server from origin/main and, only if games/deadfall changed, install them and restart
# the hub. Safe to run any time and from cron or a timer: no sudo, no changes when nothing is new, one run at a time.
#
#   deploy/update.sh            update if games/deadfall changed
#   deploy/update.sh --force    rebuild and reinstall regardless
#
# Environment (all optional):
#   DEADFALL_REPO     the BlueEngineGames checkout to fetch through   (default ~/BlueEngineGames)
#   DEADFALL_BUILD    a separate worktree to build in, kept next to the engine so the
#                     ../../../BlueEngine path dependency resolves    (default ~/BlueEngineGames-deploy)
#   DEADFALL_HOME     where the binaries are installed                (default ~/deadfall)
#
# Restarting the hub ends the rooms that are running (players drop back to the menu), so prefer quiet hours. Only
# games/deadfall is watched: after changing the engine in ~/BlueEngine run this with --force.
set -euo pipefail

REPO="${DEADFALL_REPO:-$HOME/BlueEngineGames}"
BUILD="${DEADFALL_BUILD:-$HOME/BlueEngineGames-deploy}"
DEST="${DEADFALL_HOME:-$HOME/deadfall}"
UNIT=deadfall-hub.service
FORCE=0
[ "${1:-}" = "--force" ] && FORCE=1

mkdir -p "$DEST" "$HOME/.cache" "$HOME/.local/share/deadfall"

# One run at a time.
exec 9>"$HOME/.cache/deadfall-update.lock"
flock -n 9 || { echo "another update is running"; exit 0; }

git -C "$REPO" fetch --quiet origin main
new_tree=$(git -C "$REPO" rev-parse origin/main:games/deadfall)
old_tree=$(cat "$DEST/.deployed-tree" 2>/dev/null || true)

if [ "$FORCE" = 0 ] && [ "$new_tree" = "$old_tree" ] && [ -x "$DEST/deadfall-hub" ] && [ -x "$DEST/deadfall-server" ]; then
  echo "up to date (games/deadfall tree ${new_tree:0:12})"
  exit 0
fi

# A detached worktree of origin/main, so the working copy in $REPO is never touched.
if [ ! -e "$BUILD/.git" ]; then
  git -C "$REPO" worktree add --detach "$BUILD" origin/main
else
  git -C "$BUILD" checkout --quiet --detach origin/main
fi

# The hub and the server need no window, so build without the client feature (no graphics or audio libraries).
(cd "$BUILD/games/deadfall" && cargo build --locked --release --no-default-features --bin deadfall-hub --bin deadfall-server)
target="${CARGO_TARGET_DIR:-$BUILD/games/deadfall/target}/release"

# Install next to the running copies, then rename into place: a running binary is never overwritten in place.
for f in deadfall-hub deadfall-server; do
  install -m 755 "$target/$f" "$DEST/$f.new"
  mv -f "$DEST/$f.new" "$DEST/$f"
done
install -m 755 "$BUILD/games/deadfall/deploy/deadfall-ddns.sh" "$DEST/deadfall-ddns.sh"
# The router-mapping helper comes from the engine checkout that sits beside the games repo.
if [ -f "$BUILD/../BlueEngine/tools/blue_portmap.py" ]; then
  install -m 644 "$BUILD/../BlueEngine/tools/blue_portmap.py" "$DEST/blue_portmap.py"
fi
echo "$new_tree" > "$DEST/.deployed-tree"
echo "installed games/deadfall tree ${new_tree:0:12} into $DEST"

if systemctl --user is-enabled --quiet "$UNIT" 2>/dev/null; then
  systemctl --user restart "$UNIT"
  echo "restarted $UNIT"
else
  echo "$UNIT is not enabled; not restarting (see deploy/README.md)"
fi
