#!/usr/bin/env bash
# Copies the PNGs in <dir> to <path> on the orphan `screenshots` branch, which
# is one force-pushed commit so replaced images do not pile up in history.
# Also drops pr/<n>/ of pull requests no longer open, and older heads of the
# same pull request when <path> is pr/<n>/<sha>.
# Usage: publish.sh <dir> <path>   (needs GH_TOKEN with contents: write, GITHUB_REPOSITORY)
set -euo pipefail
src=$(cd "$1" && pwd)
dest=$2
repo="https://x-access-token:${GH_TOKEN:?}@github.com/${GITHUB_REPOSITORY:?}.git"
open=$(gh pr list -R "$GITHUB_REPOSITORY" --state open --limit 1000 --json number --jq '.[].number')
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

for attempt in 1 2 3 4 5; do
  rm -rf "$work/tree" && git init -q "$work/tree" && cd "$work/tree"
  lease=
  if git fetch -q --depth=1 "$repo" refs/heads/screenshots 2>/dev/null; then
    git checkout -q --detach FETCH_HEAD
    lease=$(git rev-parse HEAD)
  fi
  for d in pr/*/; do
    [ -d "$d" ] || continue
    n=$(basename "$d")
    grep -qx "$n" <<<"$open" || rm -rf "$d"
  done
  case $dest in pr/*/*) rm -rf "$(dirname "$dest")" ;; esac
  mkdir -p "$dest"
  cp "$src"/*.png "$dest"/
  git add -A
  commit=$(git -c user.name="github-actions[bot]" -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
    commit-tree "$(git write-tree)" -m "screenshots: $dest")
  if git push -q --force-with-lease="refs/heads/screenshots:$lease" "$repo" "$commit:refs/heads/screenshots"; then
    echo "published $dest"
    exit 0
  fi
  echo "the screenshots branch moved; retrying ($attempt)" >&2
  sleep $((attempt * 3))
done
echo "could not push to the screenshots branch after 5 attempts" >&2
exit 1
