# Releasing

## Workflows

| Workflow | Runs on | Does |
| --- | --- | --- |
| `package.yml` | pull requests touching packaging, each release, by hand | optimized builds and the unsigned packages for every platform, each checked headlessly (`--check`) against the app it ships |
| `release.yml` | `just release` | the release, below |

Every action is pinned to a commit SHA, every job declares the permissions it
needs and no more, checkouts never keep credentials, and no job that feeds a
release uses a cache.

## Making a release

1. Set `version` under `[workspace.package]` in `Cargo.toml` (and
   `CFBundleShortVersionString` in `crates/moonmap-app/Info.plist`) and
   merge it to `main`. The release is tagged `v<version>` and refuses a
   version that already has a tag or a release.
2. On an up-to-date, clean checkout of `main`, run `just release`.

The workflow then:

1. runs zizmor on itself and checks the requested commit is `main`'s HEAD;
2. builds every package (`package.yml`) while clippy, the tests and the
   app's `--check` run on the same commit; nothing below runs until they
   pass;
3. signs the app with the Developer ID, notarizes and staples it, and builds
   the disk image around it, signed, notarized and stapled too; and signs
   the Windows executable with Authenticode (each when its signing is set
   up, see below);
4. writes `SHA256SUMS`, signs every file with Sigstore (keyless, as this
   workflow), and records a GitHub attestation of SLSA build provenance for
   every file;
5. verifies all of that as a user would, creates the tag and a draft release
   with generated notes, checks the uploaded files against `SHA256SUMS`, and
   publishes it.

Only the signing, attesting and publishing jobs enter the `release`
environment. Protect it (Settings → Environments) so it only accepts `main`.

## What a release contains

| File | For |
| --- | --- |
| `Moonmap-<v>-universal-apple-darwin.dmg` | macOS 13+, Apple Silicon and Intel: drag Moonmap onto the Applications shortcut |
| `Moonmap-<v>-{x86_64,aarch64}-unknown-linux-gnu.tar.gz` | Linux: a prefix (`bin/moonmap`, `share/` with the app, its icon and `.desktop` file) to copy under `/usr/local` or `~/.local` |
| `Moonmap-<v>-x86_64-pc-windows-msvc.zip` | Windows: run `moonmap.exe` from the unpacked folder |
| `SHA256SUMS` | the checksum of every file above |
| `*.sigstore.json` | the Sigstore bundle of each file |

Every package is made by `moonmap bundle` from the release executable: the
app's Luau and Moonframe's toolbox ship beside it, where it looks for them.

## Verifying a download

```sh
sha256sum --check --ignore-missing SHA256SUMS
gh attestation verify Moonmap-<v>-universal-apple-darwin.dmg --repo moonframehq/moonmap
cosign verify-blob Moonmap-<v>-universal-apple-darwin.dmg \
  --bundle Moonmap-<v>-universal-apple-darwin.dmg.sigstore.json \
  --certificate-identity https://github.com/moonframehq/moonmap/.github/workflows/release.yml@refs/heads/main \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

## Setting up macOS signing

Without it, releases still publish, but the disk image's app is signed ad hoc: Gatekeeper
refuses to open it until the user allows it in System Settings → Privacy &
Security. To sign and notarize, add to the `release` environment:

| Name | Kind | Value |
| --- | --- | --- |
| `MACOS_SIGNING_IDENTITY` | variable | `Developer ID Application: Name (TEAMID)`; setting it turns signing on |
| `MACOS_CERTIFICATE_P12_BASE64` | secret | the Developer ID Application certificate and key, exported as .p12, base64 |
| `MACOS_CERTIFICATE_PASSWORD` | secret | that .p12's password |
| `APPLE_API_KEY_ID` | variable | an App Store Connect API key's ID, for notarization |
| `APPLE_API_ISSUER_ID` | variable | its issuer ID |
| `APPLE_API_PRIVATE_KEY` | secret | its `.p8` contents |

`MACOS_SIGNING_IDENTITY` is read outside the environment too, to decide
whether to sign, so set it as a repository variable as well. The values are
the same as Moonframe's (see its `docs/releasing.md`).

The disk image opens on a laid-out window: the app and an Applications
shortcut over a background with an arrow between them, no toolbar, and the
app's icon on the volume. `scripts/release/make-dmg.sh` builds it with
dmgbuild, pinned by hash in `assets/macos/dmg/requirements.txt` and
installed as wheels into a throwaway virtualenv. The window is
`assets/macos/dmg/settings.py`; the background is drawn by
`scripts/draw-dmg-background.swift`.

The app runs under the hardened runtime with the entitlements in
`assets/macos/Moonmap.entitlements`: location (CoreLocation answers nothing
otherwise), JIT for Luau, and the microphone for dictation.

## Setting up Windows signing

Without it, the zip's `moonmap.exe` is unsigned and SmartScreen warns on
first launch. It is signed with Azure Artifact Signing by the account and
certificate profile the author's apps share. The `sign-windows` job exchanges
the `release` environment's GitHub OIDC token for an Azure token as
Moonframe's signing app registration, whose federated credential
`moonframehq-moonmap-release` trusts only
`repo:moonframehq@351295873/moonmap@1445384634:environment:release` (GitHub
names the owner and repository with their ids too, so a rename or transfer
needs a new credential). Add to the `release` environment, all as variables:

| Name | Value |
| --- | --- |
| `AZURE_CLIENT_ID` | the signing app registration's application ID |
| `AZURE_TENANT_ID` | its tenant |
| `AZURE_SUBSCRIPTION_ID` | the subscription holding the signing account |
| `ARTIFACT_SIGNING_ENDPOINT` | the signing account's regional endpoint |
| `ARTIFACT_SIGNING_ACCOUNT` | the signing account |
| `ARTIFACT_SIGNING_PROFILE` | the certificate profile |

and, as a repository variable (setting it turns signing on),
`WINDOWS_SIGNING_SUBJECT`: the certificate's exact subject, which the job
requires of every signature, with a timestamp, before it zips the folder
again.
