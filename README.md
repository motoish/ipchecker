# ipchecker

> A tiny macOS menu bar app that watches your public IPv4 address.

`ipchecker` periodically checks your public IP and compares it with an address you trust. When they differ, it highlights the mismatch and sends a macOS notification.

## Features

- Native menu bar experience with no Dock icon
- Optional live upload/download rates beside the icon (`en*` interfaces, 1s refresh, KB/s → MB/s → GB/s)
- Three fallback sources for reliable public IPv4 checks
- Expected-IP comparison and mismatch notifications
- 1, 5, 15, 30, or 60-minute check intervals
- Optional monthly CSV log of daily public IP addresses
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
- record daily public IP addresses and choose the log output folder
- include or exclude VPN addresses from future log entries
- mute notifications for the current session
- check for updates, download and verify the latest app, then reveal it in Finder for manual replacement

Configuration is stored at:

```text
~/Library/Application Support/ipchecker/config.toml
```

The network-speed, network-latency, and status-icon menu toggles are saved there as `show_network_speed`, `show_network_latency`, and `show_status_icon` (all default `true`). At least one of the three must remain enabled so the menu bar entry stays accessible. Hiding the status icon also disables all public-IP check notifications, including mismatch and fetch-failure notifications.

Daily IP logging is off by default. When enabled, choose an output folder and ipchecker creates `ipchecker-daily-global-ip-log-YYYY-MM.csv`. Each date has one row; multiple addresses observed on the same day are separated with `;`. **Record VPN Addresses** defaults to on. Turn it off to skip future observations while an active VPN tunnel carries IPv4 routes; existing CSV entries are never changed.

## Development

```bash
cargo fmt --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
```

Bundle without launching: `bash scripts/bundle-local.sh`. Extra run modes (`--verify`, `--logs`, `--telemetry`, `--debug`) are documented in `./scripts/build_and_run.sh`.

Release channels and promotion steps: [RELEASING.md](RELEASING.md).

## Notes

- Public-IP sources are queried in order: `api.ipify.org`, `ifconfig.me/ip`, then `icanhazip.com`.
- Notification permission may be requested on first use and can be changed in System Settings.
- GitHub Releases include an ad-hoc signed `ipchecker.app` zip. macOS may require right-click → Open the first time.
- In-app updates are downloaded and checksum-verified, but replacement remains manual because releases are not Developer ID signed or notarized.
- Local builds also use ad-hoc signing.
