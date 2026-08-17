#!/usr/bin/env bash
# Print the number of commits reachable from HEAD.
#
# Usage:
#   ./scripts/commit-count.sh
set -euo pipefail

if [[ $# -ne 0 ]]; then
  echo "usage: $0" >&2
  exit 2
fi

count="$(git rev-list --count HEAD)"
if [[ ! "$count" =~ ^[1-9][0-9]*$ ]]; then
  echo "invalid commit count: $count" >&2
  exit 1
fi

printf '%s\n' "$count"
