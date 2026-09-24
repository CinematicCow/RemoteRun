#!/usr/bin/env bash
# Collect recent history into a raw draft block for CHANGELOG.md.
# Usage: draft-changelog-section.sh [--since TAG] [--write] [--file PATH]
# Without --write, the draft block prints to stdout.
set -euo pipefail

since=""
write=0
file="CHANGELOG.md"

while [ $# -gt 0 ]; do
  case "$1" in
    --since) since="$2"; shift 2 ;;
    --write) write=1; shift ;;
    --file) file="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done

if [ -z "$since" ]; then
  since="$(git tag --list 'v*' --sort=-version:refname | head -n 1 || true)"
fi

if [ -n "$since" ]; then
  range="$since..HEAD"
else
  range="HEAD"
fi

entries="$(git log --reverse --format='%h %s' "$range" -- . ':!CHANGELOG.md' || true)"
if [ -z "$entries" ]; then
  echo "no commits in range $range" >&2
  exit 0
fi

block="<!-- changelog-draft:start -->"
while IFS= read -r entry; do
  hash="${entry%% *}"
  subject="${entry#* }"
  if [[ "$subject" =~ \(#([0-9]+)\) ]]; then
    pr="${BASH_REMATCH[1]}"
    ref="([#$pr](https://github.com/CinematicCow/RemoteRun/pull/$pr))"
    subject="${subject% \(#$pr\)}"
  else
    ref="(commit $hash)"
  fi
  block="$block
- $subject $ref"
done <<< "$entries"
block="$block
<!-- changelog-draft:end -->"

if [ "$write" -eq 0 ]; then
  printf '%s\n' "$block"
  exit 0
fi

python3 - "$file" "$block" <<'EOF'
import sys

path, block = sys.argv[1], sys.argv[2]
with open(path) as fh:
    text = fh.read()

start = "<!-- changelog-draft:start -->"
end = "<!-- changelog-draft:end -->"
if start in text:
    pre, _, rest = text.partition(start)
    _, _, post = rest.partition(end)
    text = pre + block + post
else:
    anchor = "## [Unreleased]\n"
    head, sep, tail = text.partition(anchor)
    if not sep:
        sys.exit("error: '## [Unreleased]' not found")
    text = head + anchor + "\n" + block + "\n" + tail

with open(path, "w") as fh:
    fh.write(text)
EOF
printf '%s\n' "$block"
