# ipchecker

> A tiny macOS menu bar app that watches your public IPv4 address.

`ipchecker` periodically checks your public IP and compares it with an address you trust. When they differ, it highlights the mismatch and sends a macOS notification.

## Features

- Native menu bar experience with no Dock icon
- Optional live upload/download rates beside the icon (`en*` interfaces, 1s refresh, KB/s → MB/s → GB/s)
- Three fallback sources for reliable public IPv4 checks
- Expected-IP comparison and mismatch notifications
- 1, 5, 15, 30, or 60-minute check intervals
- Session-only mute without hiding the warning state
- Built-in update check with verified download and Finder handoff

## Quick Start

Download `ipchecker.zip` from the [latest GitHub Release](https://github.com/motoish/ipchecker/releases/latest), unzip, and move `ipchecker.app` to Applications. The zip is ad-hoc signed; the first launch may need right-click → Open.

To build locally, you need macOS 13+, stable Rust, and Xcode Command Line Tools:

```bash
./scripts/build_and_run.sh
```

The signed local app is created at `target/release/bundle/ipchecker.app`.

## Usage

Open the menu bar icon to:

- copy the current public IP
- enter or paste an expected IPv4 address
- use the current IP as the expected address
- change the check interval or check immediately
- show or hide live upload/download rates
- mute notifications for the current session
- check for updates, download and verify the latest app, then reveal it in Finder for manual replacement

Configuration is stored at:

```text
~/Library/Application Support/ipchecker/config.toml
```

The network-speed and network-latency menu toggles are saved there as `show_network_speed` and `show_network_latency` (both default `true`).

## Development

```bash
cargo fmt --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
```

Build the app without launching it:

```bash
bash scripts/bundle-local.sh
```

Useful run modes:

```bash
./scripts/build_and_run.sh --verify
./scripts/build_and_run.sh --logs
./scripts/build_and_run.sh --telemetry
./scripts/build_and_run.sh --debug
```

## Releasing

Push to `main` runs format, Clippy, and tests first. Only if those pass does [`motoish/calver-release-action`](https://github.com/motoish/calver-release-action) publish the commit using the `Asia/Tokyo` calendar boundary.

The release channels are:

- `vYYYY.M.D-<sha8>`: immutable build pre-release with `ipchecker.zip` and `update.json`
- `vYYYY.M.D`: fast-forward-only daily pre-release channel
- `vYYYY.M`: manually promoted monthly stable release and GitHub `latest`

The app archive, update manifest, and changelog are prepared before any release records are created. CI then commits `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`, and `Info.plist` back to an unchanged `main` using an atomic lease. The same unix epoch is passed to [`motoish/calver-release-action`](https://github.com/motoish/calver-release-action), which refuses to create tags if the version does not match. `CFBundleVersion` uses the main-branch commit count. The update manifest records that numeric build, exact archive size, and SHA-256 checksum, and points to the immutable build asset.

To publish a stable update, open **Actions → Promote stable release → Run workflow** and enter an immutable tag such as `v2026.8.17-a1b2c3d4`. Promotion verifies the app archive and update manifest before creating the stable release, copies both assets to `vYYYY.M`, then verifies the uploaded files. If that monthly tag already points to another commit, promotion fails instead of replacing it.

The in-app updater follows only the manually promoted monthly `latest` release. Daily and immutable pre-releases are not offered automatically.

About shows the stable CalVer date (for example `2026.8.17`). Locales do not need edits.

Locally preview or regenerate changelog:

```bash
git-cliff --config cliff.toml -o CHANGELOG.md
```

## Notes

- Public-IP sources are queried in order: `api.ipify.org`, `ifconfig.me/ip`, then `icanhazip.com`.
- Notification permission may be requested on first use and can be changed in System Settings.
- GitHub Releases include an ad-hoc signed `ipchecker.app` zip. macOS may require right-click → Open the first time.
- In-app updates are downloaded and checksum-verified, but replacement remains manual because releases are not Developer ID signed or notarized.
- Local builds also use ad-hoc signing.
