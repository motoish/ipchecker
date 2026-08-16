#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
bundle_dir="$repo_dir/target/release/bundle/ipchecker.app"
macos_dir="$bundle_dir/Contents/MacOS"
resources_dir="$bundle_dir/Contents/Resources"
binary_path="$repo_dir/target/release/ipchecker"
info_plist="$repo_dir/resources/Info.plist"
app_icon="$repo_dir/resources/AppIcon.icns"
lsregister="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

cargo build --locked --release --manifest-path "$repo_dir/Cargo.toml" --target-dir "$repo_dir/target"

mkdir -p "$macos_dir" "$resources_dir"
cp "$binary_path" "$macos_dir/ipchecker"
cp "$info_plist" "$bundle_dir/Contents/Info.plist"
cp "$app_icon" "$resources_dir/AppIcon.icns"
printf 'APPL????' > "$bundle_dir/Contents/PkgInfo"

# Bump mtime so Finder / Launch Services pick up icon changes.
touch "$bundle_dir" "$bundle_dir/Contents/Info.plist" "$resources_dir/AppIcon.icns"

codesign --force --deep --sign - "$bundle_dir"
codesign --verify --deep --strict "$bundle_dir"

if [[ -x "$lsregister" ]]; then
  "$lsregister" -f "$bundle_dir" >/dev/null 2>&1 || true
fi

echo "bundled $bundle_dir"
