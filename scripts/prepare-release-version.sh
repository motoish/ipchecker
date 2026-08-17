#!/usr/bin/env bash
# Prepare release metadata in the current checkout without committing or tagging.
#
# Usage:
#   ./scripts/prepare-release-version.sh <YYYY.M.D-sha8> <positive-build>
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <YYYY.M.D-sha8> <positive-build>" >&2
  exit 2
fi

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"

python3 - \
  "$repo_dir/Cargo.toml" \
  "$repo_dir/Cargo.lock" \
  "$repo_dir/resources/Info.plist" \
  "$1" \
  "$2" <<'PY'
from __future__ import annotations

import datetime
import re
import sys
from pathlib import Path

cargo_toml = Path(sys.argv[1])
cargo_lock = Path(sys.argv[2])
info_plist = Path(sys.argv[3])
release_version = sys.argv[4]
build_text = sys.argv[5]

version_match = re.fullmatch(
    r"(20[0-9]{2})\.([1-9]|1[0-2])\.([1-9]|[12][0-9]|3[01])-([0-9a-f]{8})",
    release_version,
)
if not version_match:
    raise SystemExit(
        f"invalid release version {release_version!r}; expected YYYY.M.D-sha8"
    )

year, month, day = map(int, version_match.group(1, 2, 3))
try:
    datetime.date(year, month, day)
except ValueError as error:
    raise SystemExit(f"invalid release date in {release_version!r}: {error}") from error

if not re.fullmatch(r"[0-9]+", build_text):
    raise SystemExit(f"invalid build number: {build_text!r}")
build = int(build_text)
if build <= 0 or build > 2**64 - 1:
    raise SystemExit("build number must be between 1 and 2^64-1")

sha8 = version_match.group(4)
cargo_version = release_version
if sha8.isdigit() and sha8.startswith("0"):
    cargo_version = f"{year}.{month}.{day}-g{sha8}"
marketing_version = f"{year}.{month}.{day}"

cargo = cargo_toml.read_text(encoding="utf-8")
cargo, count = re.subn(
    r'(?m)^(version\s*=\s*")([^"]+)(")',
    rf"\g<1>{cargo_version}\g<3>",
    cargo,
    count=1,
)
if count != 1:
    raise SystemExit("failed to update Cargo.toml package version")
cargo_toml.write_text(cargo, encoding="utf-8")

lock = cargo_lock.read_text(encoding="utf-8")
lock, count = re.subn(
    r'(?m)^(name = "ipchecker"\nversion = ")([^"]+)(")',
    rf"\g<1>{cargo_version}\g<3>",
    lock,
    count=1,
)
if count != 1:
    raise SystemExit("failed to update Cargo.lock package version")
cargo_lock.write_text(lock, encoding="utf-8")

plist = info_plist.read_text(encoding="utf-8")
plist, short_count = re.subn(
    r"(<key>CFBundleShortVersionString</key>\s*<string>)([^<]+)(</string>)",
    rf"\g<1>{marketing_version}\g<3>",
    plist,
    count=1,
)
plist, build_count = re.subn(
    r"(<key>CFBundleVersion</key>\s*<string>)([^<]+)(</string>)",
    rf"\g<1>{build}\g<3>",
    plist,
    count=1,
)
if short_count != 1 or build_count != 1:
    raise SystemExit("failed to update Info.plist versions")
info_plist.write_text(plist, encoding="utf-8")

print(
    f"prepared release {release_version} "
    f"(Cargo {cargo_version}, CFBundleVersion {build})"
)
PY
