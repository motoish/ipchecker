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

Requires macOS 13+, stable Rust, and Xcode Command Line Tools.

```bash
./script/build_and_run.sh
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
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

Build the app without launching it:

```bash
bash scripts/bundle-local.sh
```

Useful run modes:

```bash
./script/build_and_run.sh --verify
./script/build_and_run.sh --logs
./script/build_and_run.sh --telemetry
./script/build_and_run.sh --debug
```

## Releasing

Preferred: let GitHub Actions bump versions and tag.

1. On GitHub → **Actions** → **Bump version** → **Run workflow**
2. Choose `patch` / `minor` / `major`, or type an exact `x.y.z`
3. The workflow updates `Cargo.toml` + `Info.plist`, commits to `main`, pushes `vX.Y.Z`, then runs **Release** in the same workflow (GitHub Release + `CHANGELOG.md` on `main`)

Note: a tag pushed with `GITHUB_TOKEN` does not start other workflows, so Bump calls Release via `workflow_call` instead of relying on the tag event.

About copy already uses `CARGO_PKG_VERSION`; locales do not need edits.

Locally (optional):

```bash
./scripts/bump-version.sh patch   # or: minor | major | 0.3.0
git add Cargo.toml resources/Info.plist
git commit -m "chore(release): bump version to X.Y.Z"
git tag vX.Y.Z
git push origin main vX.Y.Z
```

Locally preview or regenerate changelog:

```bash
git-cliff --config cliff.toml -o CHANGELOG.md
```

## Notes

- Public-IP sources are queried in order: `api.ipify.org`, `ifconfig.me/ip`, then `icanhazip.com`.
- Notification permission may be requested on first use and can be changed in System Settings.
- Local builds use ad-hoc signing and are intended for development, not distribution.
