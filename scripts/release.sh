#!/usr/bin/env bash
# Cut a release: bump, changelog, commit, tag, then ask to push.
set -euo pipefail

cd "$(dirname "$0")/.."

current="$(python3 - <<'PY'
import sys

in_package = False
for line in open("Cargo.toml", encoding="utf-8"):
    if line.strip() == "[package]":
        in_package = True
    elif in_package and line.startswith("["):
        break
    elif in_package and line.startswith("version = "):
        print(line.split('"')[1])
        break
else:
    sys.exit("Cargo.toml: package version not found")
PY
)"

if ! [[ "$current" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    echo "unsupported current version: $current" >&2
    exit 1
fi
auto="${BASH_REMATCH[1]}.${BASH_REMATCH[2]}.$((BASH_REMATCH[3] + 1))"

version="${1:-}"
if [ -z "$version" ]; then
    printf 'Version [%s]: ' "$auto"
    read -r version || true
    version="${version:-$auto}"
fi
version="${version#v}"

if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "invalid version: $version (use x.y.z)" >&2
    exit 1
fi

python3 - "$current" "$version" <<'PY'
import sys


def key(value):
    return tuple(int(part) for part in value.split("."))


if key(sys.argv[2]) <= key(sys.argv[1]):
    sys.exit(f"{sys.argv[2]} must be greater than {sys.argv[1]}")
PY

if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
    echo "tag v$version already exists" >&2
    exit 1
fi

if [ "$(git rev-parse --abbrev-ref HEAD)" != "master" ]; then
    echo "switch to master before releasing" >&2
    exit 1
fi

dirty="$(git status --porcelain | grep -vE '^( M|M ) CHANGELOG\.md$' || true)"
if [ -n "$dirty" ]; then
    echo "working tree has other changes:" >&2
    printf '%s\n' "$dirty" >&2
    exit 1
fi

python3 - <<'PY'
import re
import sys

text = open("CHANGELOG.md", encoding="utf-8").read()
marker = "## [Unreleased]"
if marker not in text:
    sys.exit("CHANGELOG.md: missing [Unreleased] section")
start = text.index(marker)
end = text.find("\n## [", start + 1)
section = text[start:end if end != -1 else len(text)]
bullets = [line for line in section.splitlines() if line.startswith("- ")]
if not bullets:
    sys.exit("CHANGELOG.md: [Unreleased] has no entries")
missing = [line for line in bullets if not re.search(r"(\(#\d+\)|\[#\d+\])", line)]
if missing:
    sys.exit("CHANGELOG.md: entries need a PR reference:\n" + "\n".join(missing))
PY

python3 - "$version" <<'PY'
import sys

version = sys.argv[1]
path = "Cargo.toml"
lines = open(path, encoding="utf-8").read().splitlines(keepends=True)
in_package = False
for index, line in enumerate(lines):
    if line.strip() == "[package]":
        in_package = True
    elif in_package and line.startswith("["):
        break
    elif in_package and line.startswith("version = "):
        lines[index] = f'version = "{version}"\n'
        break
else:
    sys.exit("Cargo.toml: package version not found")
open(path, "w", encoding="utf-8").writelines(lines)
PY

run_checks() {
    RR_SKIP_DASHBOARD_BUILD=1 cargo fmt-check
    RR_SKIP_DASHBOARD_BUILD=1 cargo lint
    RR_SKIP_DASHBOARD_BUILD=1 cargo test-all
}

if ! run_checks; then
    echo "checks failed. Undo with:" >&2
    echo "  git checkout -- Cargo.toml Cargo.lock CHANGELOG.md" >&2
    exit 1
fi

if ! grep -A1 '^name = "rr"$' Cargo.lock | grep -q "^version = \"$version\"$"; then
    echo "Cargo.lock was not updated to $version" >&2
    exit 1
fi

python3 - "$version" "$(date +%F)" <<'PY'
import re
import sys

version, today = sys.argv[1], sys.argv[2]
path = "CHANGELOG.md"
text = open(path, encoding="utf-8").read()
marker = "## [Unreleased]"
start = text.index(marker)
end = text.find("\n## [", start + 1)
head = text[:start]
raw = text[start + len(marker) : end if end != -1 else len(text)]
parts = re.split(r"(?m)^### (.+)$", raw)
subsections = []
for name, body in zip(parts[1::2], parts[2::2]):
    body = body.strip("\n")
    if any(line.startswith("- ") for line in body.splitlines()):
        subsections.append(f"### {name}\n\n{body}")
section = "\n\n".join(subsections)
tail = text[end + 1 :] if end != -1 else ""
skeleton = """## [Unreleased]

### Added

### Changed

### Fixed

### Removed

### Security
"""
block = f"## [{version}] - {today}\n\n{section}\n"
text = head + skeleton + "\n" + block
if tail:
    text += "\n" + tail
open(path, "w", encoding="utf-8").write(text)
PY

python3 scripts/check-changelog.py CHANGELOG.md

git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: release v$version"
git tag -a "v$version" -m "v$version"

echo
git --no-pager log -1 --oneline
echo "tag v$version"
printf 'Push master and v%s? [y/N] ' "$version"
answer=""
read -r answer || true
case "$answer" in
    y | Y | yes | YES)
        git push --atomic origin master "v$version"
        ;;
    *)
        echo "Not pushed. Push with:"
        echo "  git push --atomic origin master v$version"
        echo "Undo with:"
        echo "  git reset --hard HEAD~1 && git tag -d v$version"
        ;;
esac
