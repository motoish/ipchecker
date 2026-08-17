#!/usr/bin/env bash
# Verify that a release zip contains a usable ipchecker.app bundle.
#
# Usage:
#   ./scripts/verify-app-archive.sh <archive.zip>
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <archive.zip>" >&2
  exit 2
fi

archive="$1"
if [[ ! -s "$archive" ]]; then
  echo "app archive is missing or empty: $archive" >&2
  exit 1
fi

python3 - "$archive" <<'PY'
import plistlib
import stat
import sys
import zipfile

archive = sys.argv[1]
plist_path = "ipchecker.app/Contents/Info.plist"
binary_path = "ipchecker.app/Contents/MacOS/ipchecker"

try:
    with zipfile.ZipFile(archive) as bundle:
        corrupt = bundle.testzip()
        if corrupt is not None:
            raise SystemExit(f"app archive contains a corrupt entry: {corrupt}")
        try:
            plist_info = bundle.getinfo(plist_path)
            binary_info = bundle.getinfo(binary_path)
        except KeyError as error:
            raise SystemExit(f"app archive is missing: {error.args[0]}") from error
        if plist_info.file_size == 0 or binary_info.file_size == 0:
            raise SystemExit("app archive contains an empty plist or executable")
        mode = binary_info.external_attr >> 16
        if not stat.S_ISREG(mode) or mode & 0o111 == 0:
            raise SystemExit("app archive executable does not have an executable file mode")
        try:
            plistlib.loads(bundle.read(plist_path))
        except Exception as error:
            raise SystemExit(f"app archive contains an invalid Info.plist: {error}") from error
except zipfile.BadZipFile as error:
    raise SystemExit(f"invalid app archive: {error}") from error
PY
