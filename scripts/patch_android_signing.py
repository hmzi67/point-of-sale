#!/usr/bin/env python3
"""Wires src-tauri/gen/android/app/build.gradle.kts up to keystore.properties.

`tauri android init` (re)generates build.gradle.kts from scratch every time —
the whole src-tauri/gen/android tree is gitignored (generated output, not
committed) — and Tauri does NOT wire release signing into that generated
file on its own. Writing keystore.properties (as the release workflow's
"Configure Android signing" step does) is necessary but not sufficient: the
Gradle project has to actually read that file and apply it as the release
signingConfig, per https://v2.tauri.app/distribute/sign/android/. Without
this patch, `tauri android build` silently falls back to producing an
UNSIGNED apk/aab (e.g. app-universal-release-unsigned.apk) instead of
failing loudly.

This script applies that doc's recipe to build.gradle.kts idempotently, so
it's safe to run more than once against the same file.

Run it:
  - in CI (release.yml, android job), right after "tauri android init" and
    before "Configure Android signing"
  - locally, once after every `npm run tauri android init`, so a local
    `npm run tauri android build` also produces a signed APK/AAB (see
    DEPLOYMENT.md's Android section for the full local signing setup)

Usage:
    python3 scripts/patch_android_signing.py
"""

import pathlib
import re
import sys

GRADLE_FILE = pathlib.Path("src-tauri/gen/android/app/build.gradle.kts")

MARKER = "// >>> pos: keystore.properties signing config (scripts/patch_android_signing.py) <<<"

# Tauri's generated file already imports java.util.Properties (it uses this
# for tauri.properties) but not java.io.FileInputStream — added independently
# below, since either one could already be present.
REQUIRED_IMPORTS = ["java.util.Properties", "java.io.FileInputStream"]

# Reads the same keystore.properties fields that release.yml's "Configure
# Android signing" step writes (storeFile, password, keyAlias, keyPassword —
# "password" doubles as the store password, matching that step).
SIGNING_PATCH = f"""
    {MARKER}
    val keystorePropertiesFile = rootProject.file("keystore.properties")
    val keystoreProperties = Properties()
    if (keystorePropertiesFile.exists()) {{
        keystoreProperties.load(FileInputStream(keystorePropertiesFile))
    }}

    signingConfigs {{
        create("release") {{
            keyAlias = keystoreProperties["keyAlias"] as String
            keyPassword = keystoreProperties["keyPassword"] as String
            storeFile = file(keystoreProperties["storeFile"] as String)
            storePassword = keystoreProperties["password"] as String
        }}
    }}

    buildTypes {{
        release {{
            signingConfig = signingConfigs.getByName("release")
        }}
    }}
"""

ANDROID_BLOCK_RE = re.compile(r"(?m)^android\s*\{")


def main() -> int:
    if not GRADLE_FILE.exists():
        print(
            f"error: {GRADLE_FILE} not found — run `npm run tauri android init` first",
            file=sys.stderr,
        )
        return 1

    text = GRADLE_FILE.read_text()

    if MARKER in text:
        print(f"{GRADLE_FILE}: signing patch already present, skipping")
        return 0

    missing_imports = [
        imp for imp in REQUIRED_IMPORTS if f"import {imp}" not in text
    ]
    if missing_imports:
        text = "".join(f"import {imp}\n" for imp in missing_imports) + text

    match = ANDROID_BLOCK_RE.search(text)
    if match is None:
        print(f"error: could not find an `android {{` block in {GRADLE_FILE}", file=sys.stderr)
        return 1

    insert_at = match.end()
    text = text[:insert_at] + SIGNING_PATCH + text[insert_at:]

    GRADLE_FILE.write_text(text)
    print(f"{GRADLE_FILE}: applied signing patch")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
