#!/usr/bin/env bash
# Posts the screenshots in <dir> (published under <url>) as one comment on pull
# request <pr>, or updates the comment an earlier run posted.
# Usage: comment.sh <pr> <dir> <url> <title>   (needs GH_TOKEN, GITHUB_REPOSITORY)
set -euo pipefail
pr=$1 dir=$2 url=$3 title=$4
marker="<!-- nsh-mail-screenshots -->"
body=$(mktemp)
trap 'rm -f "$body"' EXIT
{
  echo "$marker"
  echo "$title"
  echo
  echo "| | Light | Dark |"
  echo "|---|---|---|"
  for f in "$dir"/web-{inbox,reader,compose,search,settings,add-account}-light.png; do
    [ -f "$f" ] || continue
    name=$(basename "$f" -light.png)
    label=${name#*-}
    label=${label//-/ }
    echo "| ${label^} | <img src=\"$url/$name-light.png\" width=\"420\"> | <img src=\"$url/$name-dark.png\" width=\"420\"> |"
  done
} > "$body"

api="repos/$GITHUB_REPOSITORY/issues"
id=$(gh api "$api/$pr/comments" --paginate --jq ".[] | select(.body | startswith(\"$marker\")) | .id" | head -1)
if [ -n "$id" ]; then
  gh api -X PATCH "$api/comments/$id" -F body=@"$body" --jq .html_url
else
  gh api "$api/$pr/comments" -F body=@"$body" --jq .html_url
fi
