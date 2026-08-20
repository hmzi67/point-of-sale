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

## Cutting a release

```bash
git tag v1.0.0
git push origin v1.0.0
```

Any push of a tag matching `v*` triggers the workflow. Re-pushing the same
tag cancels whatever run is already in progress for it (`concurrency`), so
force-pushing a corrected tag doesn't race a stale build.

## Where to find the artifacts

GitHub → **Releases** → the release named after your tag (e.g. `POS
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
