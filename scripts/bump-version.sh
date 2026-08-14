#!/usr/bin/env bash
# Bump the app version in every place that defines it.
#
# Single source of truth after bump: Cargo.toml `version`.
# Also syncs resources/Info.plist marketing + build numbers.
# About UI already reads env!("CARGO_PKG_VERSION") — no locale edits needed.
#
# Usage:
#   ./scripts/bump-version.sh 0.3.0
#   ./scripts/bump-version.sh patch   # 0.2.0 -> 0.2.1
#   ./scripts/bump-version.sh minor   # 0.2.0 -> 0.3.0
#   ./scripts/bump-version.sh major   # 0.2.0 -> 1.0.0
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cargo_toml="$repo_dir/Cargo.toml"
info_plist="$repo_dir/resources/Info.plist"

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <x.y.z|patch|minor|major>" >&2
  exit 2
fi

python3 - "$cargo_toml" "$info_plist" "$1" <<'PY'
from __future__ import annotations

import re
import sys
from pathlib import Path

cargo_toml = Path(sys.argv[1])
info_plist = Path(sys.argv[2])
bump = sys.argv[3]

cargo = cargo_toml.read_text(encoding="utf-8")
match = re.search(r'(?m)^version\s*=\s*"([^"]+)"', cargo)
if not match:
    raise SystemExit("version not found in Cargo.toml")
current = match.group(1)

if bump in {"patch", "minor", "major"}:
    try:
        major, minor, patch = (int(part) for part in current.split("."))
    except ValueError as error:
        raise SystemExit(f"cannot auto-bump non-semver version {current!r}") from error
    if bump == "major":
        major, minor, patch = major + 1, 0, 0
    elif bump == "minor":
        minor, patch = minor + 1, 0
    else:
        patch += 1
    version = f"{major}.{minor}.{patch}"
elif re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", bump):
    version = bump
else:
    raise SystemExit(f"invalid version: {bump} (want x.y.z, patch, minor, or major)")

cargo_new, count = re.subn(
    r'(?m)^(version\s*=\s*")([^"]+)(")',
    rf"\g<1>{version}\g<3>",
    cargo,
    count=1,
)
if count != 1:
    raise SystemExit("failed to update Cargo.toml version")
cargo_toml.write_text(cargo_new, encoding="utf-8")

plist = info_plist.read_text(encoding="utf-8")
plist, count = re.subn(
    r"(<key>CFBundleShortVersionString</key>\s*<string>)([^<]+)(</string>)",
    rf"\g<1>{version}\g<3>",
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
    build = version

plist, count = re.subn(
    r"(<key>CFBundleVersion</key>\s*<string>)([^<]+)(</string>)",
    rf"\g<1>{build}\g<3>",
    plist,
    count=1,
)
if count != 1:
    raise SystemExit("failed to update CFBundleVersion")
info_plist.write_text(plist, encoding="utf-8")

print(f"{current} -> {version} (CFBundleVersion {build})")
print("next: commit, then git tag v{0} && git push origin v{0}".format(version))

github_output = __import__("os").environ.get("GITHUB_OUTPUT")
if github_output:
    with open(github_output, "a", encoding="utf-8") as handle:
        handle.write(f"version={version}\n")
        handle.write(f"tag=v{version}\n")
PY
