# Release builds (Windows / macOS / Android)

`.github/workflows/release.yml` builds production artifacts and attaches them
all to a single GitHub release whenever a `v*` tag is pushed. iOS is
intentionally not built (out of scope for now).

## What it builds

| Job | Runner | Output |
|---|---|---|
| `windows` | `windows-latest` | Windows installer(s) (NSIS/MSI, per `tauri.conf.json` bundle targets) |
| `macos` | `macos-latest` | `.dmg`/`.app` for `aarch64-apple-darwin` and `x86_64-apple-darwin` |
| `android` | `ubuntu-latest` | Signed `.apk` and `.aab` |

A `create-release` job runs first and creates a single **draft** release for
the tag; all three build jobs upload onto that same release. A final
`finalize-release` job un-drafts it once every platform has succeeded, so the
release page never shows a partial set of artifacts.

## Secrets you need to add before pushing a tag

Settings → Secrets and variables → Actions → New repository secret.

**Required for the Android job:**

| Secret | What it is |
|---|---|
| `ANDROID_KEYSTORE_BASE64` | Your release keystore file (`.jks`), base64-encoded: `base64 -i pos-release.keystore \| pbcopy` |
| `ANDROID_KEYSTORE_PASSWORD` | The keystore's store password |
| `ANDROID_KEY_ALIAS` | The signing key's alias inside that keystore |
| `ANDROID_KEY_PASSWORD` | The signing key's password |

A `pos-release.keystore` file already exists untracked at the repo root
(per `git status`) — if that's your intended release keystore, base64-encode
it for the `ANDROID_KEYSTORE_BASE64` secret and never commit it to git.

**Not required yet — Windows and macOS builds are unsigned:**

Windows needs no secret to produce an (unsigned) installer. macOS is
deliberately left unsigned/unnotarized (no Apple Developer account yet) — the
`macos` job in the workflow has a comment block showing exactly which secrets
(`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`) and one extra step to add
later — it's a small addition, not a rewrite, once you have an account.

**Always available, no setup needed:** `GITHUB_TOKEN` is provided
automatically by GitHub Actions for creating/updating the release; the
workflow has `permissions: contents: write` so it can use it.

**Required for the Windows job:** `WEBVIEW2_FIXED_RUNTIME_URL` — see
"Windows 7 support" below. Without it, the `windows` job will fail: the
Windows build now always needs this folder present (`fixedRuntime` mode in
`tauri.conf.json` isn't conditional), not just for Win7 clients.

## Windows 7 support (WebView2 Fixed Version)

A confirmed client machine runs genuine Windows 7 SP1 (build 7601).
Microsoft's WebView2 Runtime — what Tauri uses for the whole app UI on
Windows — has had **no build newer than 109.0.1518.140 that runs on Windows
7/8/8.1 at all** since January 2023; every later release refuses to start
on those OSes. So `bundle.windows.webviewInstallMode` in `tauri.conf.json`
is pinned to:

```json
"webviewInstallMode": { "type": "fixedRuntime", "path": "./vendor/webview2-fixed-runtime-109/" }
```

This bundles that exact runtime build into the installer instead of relying
on whatever's on the target machine or downloading one at install time —
which is also what avoids the original bootstrapper crash
(`PackageIdFromFullName`/`KERNEL32.dll` entry-point error) entirely, on any
OS. `src-tauri/windows/hooks.nsh` still runs a pre-install check, but the
floor is now Windows 7 SP1, not Windows 10 — anything genuinely older
(Vista, XP) is still turned away with a plain-language message instead of a
cryptic failure partway through setup.

**Deliberate, accepted tradeoffs** (see the fuller discussion in the PR/session
this was added in if you need the full reasoning):
- v109 has had zero security patches since Jan 2023 and never will — it's
  permanently frozen for Win7 clients specifically.
- It may not fully support every WebView2 API our Tauri version (2.11.5+)
  expects; if something behaves oddly *only* on the Win7 build, this is the
  first thing to suspect.
- Every Windows build now bundles this (larger installer for all clients,
  not just Win7 ones) — it wasn't split into a separate Win7-only build.

### Where the runtime folder comes from

Microsoft's official WebView2 download page only ever serves the *current*
Fixed Version release — v109 has already aged off it and has no official
download source any more. This repo does not (and should not) vendor a
copy from an unverified third party. **You are responsible for sourcing
`Microsoft.WebView2.FixedVersionRuntime.109.0.1518.140` yourself** from a
channel you trust, for both `x64` and, if any client machine is 32-bit,
`x86`.

Once you have it:

1. Decompress it with `expand {package} -F:* {dest}` (not File Explorer —
   Microsoft's own docs warn that can produce the wrong folder structure).
2. For **local dev builds**: place the decompressed folder at
   `src-tauri/vendor/webview2-fixed-runtime-109/` (gitignored — never commit
   it, it's a large third-party binary).
3. For **CI builds**: zip that same folder and host it somewhere the
   `windows-latest` runner can fetch over HTTPS with a single `curl` (a
   private blob-storage URL with a long-lived SAS/presigned token, or a
   private GitHub release asset's authenticated download URL). Put that
   full URL in the `WEBVIEW2_FIXED_RUNTIME_URL` secret. The `windows` job
   downloads and unzips it into `src-tauri/vendor/webview2-fixed-runtime-109/`
   before the Tauri build step runs (see `release.yml`'s `windows` job).

## Cutting a release

```bash
git tag v1.0.0
git push origin v1.0.0
```

Any push of a tag matching `v*` triggers the workflow. Re-pushing the same
tag cancels whatever run is already in progress for it (`concurrency`), so
force-pushing a corrected tag doesn't race a stale build.

## Where to find the artifacts

GitHub → **Releases** → the release named after your tag (e.g. `Diwan
v1.0.0`). It stays in draft until all three platform jobs finish
successfully; once published it has the Windows installer, both macOS
builds, and the Android APK/AAB attached together.

## Things worth double-checking before your first real run

The workflow pins several versions (`tauri-apps/tauri-action@v1`, JDK 17,
Android NDK r27c, `android-actions/setup-android@v4`) based on what this
repo's `@tauri-apps/cli` version required at the time the workflow was
written — see the comment block at the top of `release.yml`. Tauri's Android
tooling requirements change between releases, so re-verify those against
[Tauri's Android prerequisites](https://v2.tauri.app/start/prerequisites/#android)
and the [tauri-action releases page](https://github.com/tauri-apps/tauri-action/releases)
if `@tauri-apps/cli` gets bumped and the Android build starts failing.

The Android job also regenerates `src-tauri/gen/android` via `tauri android
init` if it isn't already present in the checkout (it's currently untracked
in this repo). If you'd rather have CI use an exact, hand-configured Android
project instead of a freshly generated one, commit `src-tauri/gen/android`
to the repo (minus `keystore.properties`, which must stay secret) and drop
that init step.

### Android signing works locally too, not just in CI

`tauri android init` generates `src-tauri/gen/android/app/build.gradle.kts`
from scratch every time, and Tauri does not wire that file up to read
`keystore.properties` or sign the release build type on its own — see
[Tauri's Android signing guide](https://v2.tauri.app/distribute/sign/android/).
The release workflow patches this in automatically (`scripts/patch_android_signing.py`,
run right after `tauri android init`); without it, `tauri android build`
silently produces an *unsigned* APK/AAB
(`app-universal-release-unsigned.apk`) instead of a signed one.

To get the same signed output locally:

```bash
npm run tauri android init          # only needed once, or after deleting gen/android
python3 scripts/patch_android_signing.py

cat > src-tauri/gen/android/keystore.properties <<EOF
storeFile=/absolute/path/to/pos-release.keystore
password=<your keystore password>
keyAlias=<your key alias>
keyPassword=<your key password>
EOF

npm run tauri android build -- --apk --aab
```

`scripts/patch_android_signing.py` is idempotent — re-run it after any
`tauri android init` that regenerates `gen/android`; it no-ops if the patch
is already present. Never commit `keystore.properties` — it's inside the
gitignored `gen/android` tree.
