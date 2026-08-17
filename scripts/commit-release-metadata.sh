#!/usr/bin/env bash
# Commit generated release metadata and push it back to main.
#
# Usage:
#   ./scripts/commit-release-metadata.sh <YYYY.M.D-sha8> <expected-main-sha>
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <YYYY.M.D-sha8> <expected-main-sha>" >&2
  exit 2
fi

version="$1"
expected_head="$2"
python3 - "$version" <<'PY'
import datetime
import re
import sys

version = sys.argv[1]
match = re.fullmatch(
    r"(20[0-9]{2})\.([1-9]|1[0-2])\.([1-9]|[12][0-9]|3[01])-([0-9a-f]{8})",
    version,
)
if not match:
    raise SystemExit(f"invalid release version: {version!r}")
try:
    datetime.date(*map(int, match.group(1, 2, 3)))
except ValueError as error:
    raise SystemExit(f"invalid release version: {version!r}: {error}") from error
PY

if [[ ! "$expected_head" =~ ^[0-9a-f]{40}$ ]]; then
  echo "invalid expected main SHA: $expected_head" >&2
  exit 1
fi

repo_dir="$(git rev-parse --show-toplevel)"
cd "$repo_dir"

git fetch --no-tags origin main
remote_head="$(git rev-parse refs/remotes/origin/main)"
local_head="$(git rev-parse HEAD)"
if [[ "$local_head" != "$expected_head" ]]; then
  echo "main changed during release: expected $expected_head, local $local_head, remote $remote_head" >&2
  exit 1
fi
if ! git diff --cached --quiet; then
  echo "index must be clean before committing release metadata" >&2
  exit 1
fi

files=(
  CHANGELOG.md
  Cargo.lock
  Cargo.toml
  resources/Info.plist
)
for file in "${files[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "release metadata file is missing: $file" >&2
    exit 1
  fi
done

is_same_as_remote=true
for file in "${files[@]}"; do
  if ! git diff --quiet "refs/remotes/origin/main" -- "$file"; then
    is_same_as_remote=false
    break
  fi
done

if [[ "$remote_head" != "$expected_head" ]]; then
  if [[ "$is_same_as_remote" == true ]]; then
    echo "release metadata already on main" >&2
    exit 0
  fi
  echo "main changed during release: expected $expected_head, local $local_head, remote $remote_head" >&2
  exit 1
fi

git add -- "${files[@]}"
if git diff --cached --quiet; then
  echo "no release metadata changes to commit" >&2
  exit 1
fi

actual="$(git diff --cached --name-only | LC_ALL=C sort)"
while IFS= read -r staged; do
  if [[ -z "$staged" ]]; then
    continue
  fi
  is_allowed=false
  for file in "${files[@]}"; do
    if [[ "$staged" == "$file" ]]; then
      is_allowed=true
      break
    fi
  done
  if [[ "$is_allowed" != true ]]; then
    echo "unexpected staged release files:" >&2
    printf '%s\n' "$actual" >&2
    exit 1
  fi
done <<< "$actual"

git config --local user.name "github-actions[bot]"
git config --local user.email "41898282+github-actions[bot]@users.noreply.github.com"
git commit --no-gpg-sign -m "chore(release): bump version to $version [skip ci]"
git push --force-with-lease="refs/heads/main:$expected_head" origin HEAD:main
