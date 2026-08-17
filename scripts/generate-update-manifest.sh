#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 6 ]]; then
  echo "usage: $0 <asset> <version> <build> <tag> <repository> <output>" >&2
  exit 2
fi

python3 - "$1" "$2" "$3" "$4" "$5" "$6" <<'PY'
from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

asset = Path(sys.argv[1])
version = sys.argv[2]
build_text = sys.argv[3]
tag = sys.argv[4]
repository = sys.argv[5]
output = Path(sys.argv[6])

if not asset.is_file():
    raise SystemExit(f"update asset does not exist: {asset}")
if not re.fullmatch(r"[0-9A-Za-z.-]+", version):
    raise SystemExit(f"invalid update version: {version!r}")
if tag != f"v{version}":
    raise SystemExit(f"tag {tag!r} does not match version {version!r}")
if not re.fullmatch(r"[0-9A-Za-z_.-]+/[0-9A-Za-z_.-]+", repository):
    raise SystemExit(f"invalid GitHub repository: {repository!r}")
try:
    build = int(build_text)
except ValueError as error:
    raise SystemExit(f"invalid build number: {build_text!r}") from error
if build <= 0:
    raise SystemExit("build number must be positive")

digest = hashlib.sha256()
with asset.open("rb") as handle:
    for chunk in iter(lambda: handle.read(1024 * 1024), b""):
        digest.update(chunk)

manifest = {
    "version": version,
    "build": build,
    "url": f"https://github.com/{repository}/releases/download/{tag}/{asset.name}",
    "size": asset.stat().st_size,
    "sha256": digest.hexdigest(),
}
output.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
