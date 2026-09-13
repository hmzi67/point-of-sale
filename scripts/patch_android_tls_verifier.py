#!/usr/bin/env python3
"""Adds rustls-platform-verifier's Kotlin component to src-tauri/gen/android.

The licence client (src-tauri/src/license/client.rs) talks HTTPS through
reqwest, which verifies certificates with rustls-platform-verifier. On
Android that crate calls into a small Kotlin class via JNI, and that class
has to be part of the APK. It is not on Maven Central: it ships *inside the
Rust crate* (`rustls-platform-verifier-android`) as a local Maven repository.
Without it — or with R8 stripping it, since nothing but JNI references it —
licence activation on Android cannot verify the server's certificate.

`tauri android init` regenerates build.gradle.kts from scratch and the whole
src-tauri/gen/android tree is gitignored, so this has to be re-applied after
every init — same as patch_android_signing.py and patch_android_manifest.py.

The crate's on-disk location is resolved with `cargo metadata` at patch time
and written as an absolute path. Re-running replaces the block, so it stays
correct after a cargo registry move or a crate version bump.

Run it:
  - in CI (release.yml, android job), after "tauri android init"
  - locally, once after every `npm run tauri android init`

Usage:
    python3 scripts/patch_android_tls_verifier.py
"""

import json
import pathlib
import re
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
GRADLE_FILE = REPO / "src-tauri/gen/android/app/build.gradle.kts"
PROGUARD_FILE = REPO / "src-tauri/gen/android/app/proguard-rules.pro"
CARGO_MANIFEST = str(REPO / "src-tauri/Cargo.toml")

BEGIN = "// >>> pos: rustls-platform-verifier component (scripts/patch_android_tls_verifier.py) >>>"
END = "// <<< pos: rustls-platform-verifier component <<<"
BLOCK_RE = re.compile(re.escape(BEGIN) + r".*?" + re.escape(END) + r"\n?", re.S)

PROGUARD_RULE = "-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }"


def find_verifier_android() -> tuple[pathlib.Path, str]:
    out = subprocess.run(
        [
            "cargo", "metadata", "--format-version", "1",
            "--filter-platform", "aarch64-linux-android",
            "--manifest-path", CARGO_MANIFEST,
        ],
        check=True, capture_output=True, text=True,
    ).stdout
    for package in json.loads(out)["packages"]:
        if package["name"] == "rustls-platform-verifier-android":
            maven = pathlib.Path(package["manifest_path"]).parent / "maven"
            return maven, package["version"]
    raise SystemExit("error: rustls-platform-verifier-android is not in the Android dependency tree")


def main() -> int:
    if not GRADLE_FILE.exists():
        print(f"error: {GRADLE_FILE} not found — run `npm run tauri android init` first", file=sys.stderr)
        return 1

    maven, version = find_verifier_android()
    if not maven.is_dir():
        print(f"error: expected the verifier's Maven repo at {maven}", file=sys.stderr)
        return 1

    block = f"""{BEGIN}
repositories {{
    maven {{
        // Default metadata sources (the .pom): it declares <packaging>aar</packaging>.
        // `metadataSources {{ artifact() }}` skips the pom and makes Gradle look
        // for a .jar that does not exist — "Could not find rustls:rustls-platform-verifier".
        url = uri({json.dumps(maven.as_posix())})
    }}
}}

dependencies {{
    implementation("rustls:rustls-platform-verifier:{version}")
}}
{END}
"""

    text = BLOCK_RE.sub("", GRADLE_FILE.read_text())
    GRADLE_FILE.write_text(text.rstrip("\n") + "\n\n" + block)
    print(f"{GRADLE_FILE}: applied TLS verifier patch (rustls-platform-verifier {version})")

    rules = PROGUARD_FILE.read_text() if PROGUARD_FILE.exists() else ""
    if PROGUARD_RULE not in rules:
        PROGUARD_FILE.write_text(rules.rstrip("\n") + "\n\n# rustls-platform-verifier is only reached via JNI\n" + PROGUARD_RULE + "\n")
        print(f"{PROGUARD_FILE}: added keep rule")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
