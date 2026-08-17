#!/usr/bin/env bash
# Compute the immutable CalVer identity used by motoish/calver-release-action.
#
# Usage:
#   ./scripts/calver-identity.sh <timezone> <sha> [unix_epoch]
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "usage: $0 <timezone> <sha> [unix_epoch]" >&2
  exit 2
fi

python3 - "$1" "$2" "${3:-}" <<'PY'
from __future__ import annotations

import os
import re
import sys
from datetime import datetime, timezone
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

timezone_name = sys.argv[1]
sha = sys.argv[2]
epoch_text = sys.argv[3]

if not re.fullmatch(r"[0-9a-f]{40}", sha):
    raise SystemExit(f"invalid commit SHA: {sha!r}")

try:
    zone = ZoneInfo(timezone_name)
except ZoneInfoNotFoundError as error:
    raise SystemExit(f"invalid IANA timezone: {timezone_name}") from error

if epoch_text == "":
    now = datetime.now(timezone.utc)
else:
    if not re.fullmatch(r"[0-9]+", epoch_text):
        raise SystemExit(f"invalid unix epoch: {epoch_text!r}")
    now = datetime.fromtimestamp(int(epoch_text), timezone.utc)

local = now.astimezone(zone)
epoch = int(now.timestamp())
version = f"{local.year}.{local.month}.{local.day}-{sha[:8]}"
build_tag = f"v{version}"
output = f"version={version}\nbuild_tag={build_tag}\nepoch={epoch}\n"
sys.stdout.write(output)

github_output = os.environ.get("GITHUB_OUTPUT")
if github_output:
    with open(github_output, "a", encoding="utf-8") as handle:
        handle.write(output)
PY
