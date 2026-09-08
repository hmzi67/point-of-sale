#!/usr/bin/env python3
"""Adds the printer hardware declarations to src-tauri/gen/android/.../AndroidManifest.xml.

`tauri android init` regenerates the manifest from scratch every time — the
whole src-tauri/gen/android tree is gitignored (generated output, not
committed) — and its template declares only INTERNET. That is a problem for
both Android printer transports:

  * **Bluetooth** (printer/android_bt.rs). `BluetoothAdapter.isEnabled()`
    requires BLUETOOTH on API < 31 and BLUETOOTH_CONNECT on 31+. With
    neither declared, it throws a bare SecurityException, which JNI surfaces
    with nothing in it pointing at the cause — the long-standing
    "isEnabled check failed: Java exception" on real client devices. Note
    that declaring BLUETOOTH_CONNECT is also a *precondition* for requesting
    it at runtime: an undeclared runtime permission can never be granted, so
    no amount of asking would have fixed this on its own.

  * **USB-OTG** (printer/android_usb.rs). USB host access needs no manifest
    permission — the per-device runtime grant is the whole model — but the
    device must actually support USB host mode, declared as a uses-feature.
    It is declared required="false" on purpose: this is one printer
    transport of several, not a prerequisite for running the app, and
    required="true" would make the app uninstallable (and Play-Store-hidden)
    on any phone without OTG.

Same shape and lifecycle as scripts/patch_android_signing.py, and idempotent
for the same reason: safe to run more than once against the same file.

Run it:
  - in CI (release.yml, android job), right after "tauri android init"
  - locally, once after every `npm run tauri android init`

Usage:
    python3 scripts/patch_android_manifest.py
"""

import pathlib
import sys

MANIFEST = pathlib.Path("src-tauri/gen/android/app/src/main/AndroidManifest.xml")

MARKER = "<!-- >>> pos: printer hardware (scripts/patch_android_manifest.py) <<< -->"

# maxSdkVersion on the two legacy Bluetooth permissions because they were
# replaced by BLUETOOTH_CONNECT in API 31 — keeping them unbounded makes
# Play Console flag them as unnecessary, and the OS ignores them there
# anyway.
BLOCK = f"""    {MARKER}
    <uses-permission android:name="android.permission.BLUETOOTH" android:maxSdkVersion="30" />
    <uses-permission android:name="android.permission.BLUETOOTH_ADMIN" android:maxSdkVersion="30" />
    <uses-permission android:name="android.permission.BLUETOOTH_CONNECT" />
    <uses-feature android:name="android.hardware.usb.host" android:required="false" />
"""

ANCHOR = '<uses-permission android:name="android.permission.INTERNET" />'


def main() -> int:
    if not MANIFEST.exists():
        print(f"error: {MANIFEST} not found — run `npm run tauri android init` first", file=sys.stderr)
        return 1

    text = MANIFEST.read_text()

    if MARKER in text:
        print(f"{MANIFEST}: already patched, nothing to do")
        return 0

    if ANCHOR not in text:
        print(f"error: {MANIFEST} has no <uses-permission INTERNET> line to anchor to", file=sys.stderr)
        return 1

    text = text.replace(ANCHOR, ANCHOR + "\n\n" + BLOCK.rstrip(), 1)
    MANIFEST.write_text(text)
    print(f"{MANIFEST}: added Bluetooth permissions + USB host feature")
    return 0


if __name__ == "__main__":
    sys.exit(main())
