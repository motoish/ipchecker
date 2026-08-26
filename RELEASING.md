# Releasing

Push to `main` runs format, Clippy, and tests first. Only if those pass does [`motoish/calver-release-action`](https://github.com/motoish/calver-release-action) publish the commit using the `Asia/Tokyo` calendar boundary.

## Channels

- `vYYYY.M.D-<sha8>`: immutable build pre-release with `ipchecker.zip` and `update.json`
- `vYYYY.M.D`: fast-forward-only daily pre-release channel
- `vYYYY.M`: manually promoted monthly stable release and GitHub `latest`

## Pipeline

The app archive, update manifest, and changelog are prepared before any release records are created. CI then commits `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`, and `Info.plist` back to an unchanged `main` using an atomic lease. The same unix epoch is passed to [`motoish/calver-release-action`](https://github.com/motoish/calver-release-action), which refuses to create tags if the version does not match. `CFBundleVersion` uses the main-branch commit count. The update manifest records that numeric build, exact archive size, and SHA-256 checksum, and points to the immutable build asset.

## Promote stable

To publish a stable update, open **Actions → Promote stable release → Run workflow** and enter an immutable tag such as `v2026.8.17-a1b2c3d4`. Promotion verifies the app archive and update manifest, summarizes all changes since the previous monthly stable release under a single `YYYY.M` heading, copies both assets to `vYYYY.M`, then verifies the uploaded files. If that monthly tag already points to another commit, promotion fails instead of replacing it.

The in-app updater follows only the manually promoted monthly `latest` release. Daily and immutable pre-releases are not offered automatically.

About shows the stable CalVer date (for example `2026.8.17`). Locales do not need edits.

## Changelog preview

Locally preview or regenerate changelog:

```bash
git-cliff --config cliff.toml -o CHANGELOG.md
```
