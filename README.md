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

Configuration is stored at:

```text
~/Library/Application Support/ipchecker/config.toml
```

The network-speed menu toggle is saved there as `show_network_speed` (default `true`).

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

Push to `main` runs format, Clippy, and tests first. Only if those pass does it create a GitHub Release. One push is one release, using the last commit.

Each push creates an immutable pre-release `YYYY.M.D-<sha8>` (for example `2026.8.16-a1b2c3d4`) and moves that day's stable release `v2026.8.16` to the same commit. GitHub's latest release is the daily stable tag. Cargo versions cannot use `_`, so the unique tag uses `-`.

The workflow updates `Cargo.toml`, `Cargo.lock`, `Info.plist`, and `CHANGELOG.md` in one commit, tags the unique pre-release and that day's stable tag, then creates the GitHub Releases with a zipped `ipchecker.app`.

About shows the stable CalVer date (for example `2026.8.17`). Locales do not need edits.

Locally preview or regenerate changelog:

```bash
git-cliff --config cliff.toml -o CHANGELOG.md
```

## Notes

- Public-IP sources are queried in order: `api.ipify.org`, `ifconfig.me/ip`, then `icanhazip.com`.
- Notification permission may be requested on first use and can be changed in System Settings.
- GitHub Releases include an ad-hoc signed `ipchecker.app` zip. macOS may require right-click → Open the first time.
- Local builds also use ad-hoc signing.
