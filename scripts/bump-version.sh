#!/usr/bin/env bash
# Bump the app version in every place that defines it.
#
# Single source of truth after bump: Cargo.toml `version`.
# Also syncs Cargo.lock and resources/Info.plist. CFBundleShortVersionString is
# the stable CalVer `YYYY.M.D` (Apple requires three numeric segments).
# CFBundleVersion is a monotonic integer build. About UI reads the date prefix
# of CARGO_PKG_VERSION — no locale edits needed.
#
# Releases use CalVer as Cargo-compatible `YYYY.M.D-<sha8>` (no leading zeros),
# for example 2026.8.16-a1b2c3d4. The calendar day is Asia/Tokyo. Cargo
# versions cannot contain `_`, so the hash is separated with `-`.
#
# Usage:
#   ./scripts/bump-version.sh
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cargo_toml="$repo_dir/Cargo.toml"
cargo_lock="$repo_dir/Cargo.lock"
info_plist="$repo_dir/resources/Info.plist"

if [[ $# -ne 0 ]]; then
  echo "usage: $0" >&2
  exit 2
fi

python3 - "$cargo_toml" "$cargo_lock" "$info_plist" <<'PY'
from __future__ import annotations

import os
import re
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from zoneinfo import ZoneInfo

cargo_toml = Path(sys.argv[1])
cargo_lock = Path(sys.argv[2])
info_plist = Path(sys.argv[3])

cargo = cargo_toml.read_text(encoding="utf-8")
match = re.search(r'(?m)^version\s*=\s*"([^"]+)"', cargo)
if not match:
    raise SystemExit("version not found in Cargo.toml")
current = match.group(1)


def today_calver() -> str:
    now = datetime.now(ZoneInfo("Asia/Tokyo"))
    return f"{now.year}.{now.month}.{now.day}"


def git_head_sha() -> str:
    sha = os.environ.get("GITHUB_SHA", "").strip()
    if sha:
        return sha
    return subprocess.check_output(
        ["git", "rev-parse", "HEAD"],
        text=True,
    ).strip()


def prerelease_sha(sha: str) -> str:
    sha8 = sha[:8].lower()
    if not re.fullmatch(r"[0-9a-f]{8}", sha8):
        raise SystemExit(f"commit hash is too short or not hex: {sha!r}")
    if sha8.isdigit() and sha8.startswith("0"):
        return f"g{sha8}"
    return sha8


stable = today_calver()
version = f"{stable}-{prerelease_sha(git_head_sha())}"

if version == current:
    raise SystemExit(f"already at version {current}")

cargo_new, count = re.subn(
    r'(?m)^(version\s*=\s*")([^"]+)(")',
    rf"\g<1>{version}\g<3>",
    cargo,
    count=1,
)
if count != 1:
    raise SystemExit("failed to update Cargo.toml version")
cargo_toml.write_text(cargo_new, encoding="utf-8")

lock = cargo_lock.read_text(encoding="utf-8")
lock_new, count = re.subn(
    r'(?m)^(name = "ipchecker"\nversion = ")([^"]+)(")',
    rf"\g<1>{version}\g<3>",
    lock,
    count=1,
)
if count != 1:
    raise SystemExit("failed to update Cargo.lock version")
cargo_lock.write_text(lock_new, encoding="utf-8")

plist = info_plist.read_text(encoding="utf-8")
plist, count = re.subn(
    r"(<key>CFBundleShortVersionString</key>\s*<string>)([^<]+)(</string>)",
    rf"\g<1>{stable}\g<3>",
    plist,
    count=1,
)
if count != 1:
    raise SystemExit("failed to update CFBundleShortVersionString")

build_match = re.search(
    r"<key>CFBundleVersion</key>\s*<string>([^<]+)</string>",
    plist,
)
if not build_match:
    raise SystemExit("CFBundleVersion not found")
try:
    build = str(int(build_match.group(1)) + 1)
except ValueError:
    build = "1"

plist, count = re.subn(
    r"(<key>CFBundleVersion</key>\s*<string>)([^<]+)(</string>)",
    rf"\g<1>{build}\g<3>",
    plist,
    count=1,
)
if count != 1:
    raise SystemExit("failed to update CFBundleVersion")
info_plist.write_text(plist, encoding="utf-8")

print(f"{current} -> {version} (CFBundleShortVersionString {stable}, CFBundleVersion {build})")

github_output = os.environ.get("GITHUB_OUTPUT")
if github_output:
    with open(github_output, "a", encoding="utf-8") as handle:
        handle.write(f"version={version}\n")
        handle.write(f"tag=v{version}\n")
        handle.write(f"stable_tag=v{today_calver()}\n")
PY
