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
  - `workflow_dispatch`: build-only dry run that uploads workflow artifacts for inspection.
  - `push` of `vX.Y.Z`: builds every target and creates a draft GitHub Release with the generated bundles attached.

## Release Checklist

1. Update the version in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`.
2. Merge the version bump into the branch you want to release from.
3. Trigger `Release` with `workflow_dispatch` once to verify the matrix and inspect uploaded artifacts.
4. Push a matching tag such as `v1.0.0` on the release commit.
5. Wait for the draft GitHub Release to appear and verify the attached assets.
6. Publish the draft release once installers have been spot-checked.

The release workflow fails early if the three version declarations do not match, or if the pushed tag does not match `src-tauri/tauri.conf.json`.

## macOS Signing

If no Apple signing secrets are configured, the workflow falls back to ad-hoc signing by setting `APPLE_SIGNING_IDENTITY=-`. This keeps Apple Silicon downloads runnable, but users should expect the normal macOS security warning for unsigned direct downloads.

To enable full macOS signing in CI, configure:

- `APPLE_CERTIFICATE`: base64-encoded `.p12` signing certificate
- `APPLE_CERTIFICATE_PASSWORD`: password for the `.p12`
- `KEYCHAIN_PASSWORD`: temporary CI keychain password
- `APPLE_SIGNING_IDENTITY`: optional explicit identity name; if omitted, the workflow auto-detects the first suitable identity from the imported certificate

For notarization, configure one of these sets:

- App Store Connect API:
  - `APPLE_API_ISSUER`
  - `APPLE_API_KEY`
  - `APPLE_API_KEY_CONTENT`: raw `.p8` private key contents
- Apple ID notarization:
  - `APPLE_ID`
  - `APPLE_PASSWORD`
  - `APPLE_TEAM_ID`

## Windows Signing

Windows installer signing is intentionally deferred in v1. The workflow is structured so a future `signCommand`-based Tauri configuration or Azure Trusted Signing integration can be added without changing the release contract.

## Notes

- GitHub Releases are the distribution source of truth for v1.
- The project does not enable Tauri updater artifacts yet.
- Linux bundles are built on `ubuntu-22.04` with the Tauri system dependencies installed in CI.
