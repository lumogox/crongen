# Release Process

crongen ships cross-platform desktop bundles through GitHub Actions and GitHub Releases.

## Release Outputs

- macOS Apple Silicon: `.dmg`
- macOS Intel: `.dmg`
- Windows x64: NSIS setup `.exe`
- Linux x64: `.AppImage`
- Linux x64: `.deb`

The release workflow intentionally does not build MSI, RPM, or updater artifacts in v1.

## Workflows

- [`CI`](../.github/workflows/ci.yml) runs on pushes and pull requests. It installs dependencies, runs TypeScript and Rust checks, and verifies the production frontend build.
- [`Release`](../.github/workflows/release.yml) runs in two modes:
  - `workflow_dispatch`: build-only dry run that uploads workflow artifacts for inspection. You can optionally provide a version override such as `1.0.4`.
  - `push` of `vX.Y.Z`: builds every target and uploads the generated bundles into the existing GitHub Release for that tag, or creates a draft release first if one does not exist yet.

## Release Checklist

1. Merge the commit you want to release into the branch you want to release from.
2. Trigger `Release` with `workflow_dispatch` once to verify the matrix. Optionally provide the intended version to validate bundle naming before tagging.
3. Create a new tag such as `v1.0.4` on the release commit, or create a draft release in GitHub with that tag.
4. Wait for the `Release` workflow to finish and verify the attached assets.
5. Publish the draft release once installers have been spot-checked.

The release workflow now treats the tag as the release version source of truth for tagged releases. It validates the tag format, then overrides the Tauri app version during CI with `tauri build --config ...`, so you do not need to edit `package.json`, `src-tauri/Cargo.toml`, or `src-tauri/tauri.conf.json` just to cut a new release tag.

When no version override is provided, `workflow_dispatch` still checks that the repository's version declarations match each other.

## macOS Signing

The release workflow currently builds macOS DMGs with `--no-sign`, so no Apple signing or notarization secrets are required in CI.

That keeps the pipeline simple and avoids certificate import failures, but macOS downloads should be treated as unsigned direct downloads and may require user approval in Privacy & Security before first launch.

## Windows Signing

Windows installer signing is intentionally deferred in v1. The workflow is structured so a future `signCommand`-based Tauri configuration or Azure Trusted Signing integration can be added without changing the release contract.

## Notes

- GitHub Releases are the distribution source of truth for v1.
- The project does not enable Tauri updater artifacts yet.
- Linux bundles are built on `ubuntu-22.04` with the Tauri system dependencies installed in CI.
- `src-tauri/tauri.conf.json` is still required as Tauri's main configuration file. The release workflow only overrides the `version` field at build time.
