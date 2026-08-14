#!/usr/bin/env bash
# Generate CHANGELOG.md with git-cliff (same config as CI).
#
# Usage:
#   ./scripts/generate-changelog.sh
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_dir"

if ! command -v git-cliff >/dev/null 2>&1; then
  echo "git-cliff is required (https://git-cliff.org)" >&2
  exit 1
fi

git-cliff --config cliff.toml -o CHANGELOG.md
echo "wrote $repo_dir/CHANGELOG.md"
