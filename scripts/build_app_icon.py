#!/usr/bin/env python3
"""Build resources/AppIcon.icns from resources/ipchecker-icon.png."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    from PIL import Image
except ImportError as error:
    raise SystemExit(
        "Pillow is required to build AppIcon.icns (pip install pillow)"
    ) from error

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "resources" / "ipchecker-icon.png"
OUT = ROOT / "resources" / "AppIcon.icns"

# Assemble "@2x.png" at runtime so the source is not rewritten by email redaction.
AT2X = f"{chr(64)}2x.png"

SIZES = (
    ("icon_16x16.png", 16),
    (f"icon_16x16{AT2X}", 32),
    ("icon_32x32.png", 32),
    (f"icon_32x32{AT2X}", 64),
    ("icon_128x128.png", 128),
    (f"icon_128x128{AT2X}", 256),
    ("icon_256x256.png", 256),
    (f"icon_256x256{AT2X}", 512),
    ("icon_512x512.png", 512),
    (f"icon_512x512{AT2X}", 1024),
)


def main() -> int:
    if not SRC.is_file():
        print(f"missing source icon: {SRC}", file=sys.stderr)
        return 1

    src = Image.open(SRC).convert("RGBA")
    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "AppIcon.iconset"
        iconset.mkdir()
        for name, size in SIZES:
            path = iconset / name
            src.resize((size, size), Image.Resampling.LANCZOS).save(path, format="PNG")
            got = Image.open(path)
            if got.size != (size, size) or got.mode != "RGBA":
                print(f"bad slice {name}: {got.size} {got.mode}", file=sys.stderr)
                return 1

        names = sorted(p.name for p in iconset.iterdir())
        expected = sorted(name for name, _ in SIZES)
        if names != expected:
            print(f"iconset mismatch:\n  got {names}\n  want {expected}", file=sys.stderr)
            return 1

        OUT.parent.mkdir(parents=True, exist_ok=True)
        subprocess.check_call(["iconutil", "-c", "icns", str(iconset), "-o", str(OUT)])

    print(f"wrote {OUT} ({OUT.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
