# Vendor access — recovery & operational notes

This document is for the product's vendor/developer only. It is not
client-facing and describes a hidden feature that must stay undiscoverable
through normal use of the app — do not reference this file, the entry
point, or the account it describes in anything a client sees.

## What this is

Every installation has a second, separate account — not a row in `users`,
not visible in Manage Users, not reachable from the normal login screen —
that gives the vendor an override path to lock or unlock any module
(`enabled_modules.desktop_locked` / `android_locked`), independent of and
overriding the client's own Owner/Admin toggles in Settings.

- **Storage**: `product_owner_account` table, one row (`id = 1`), holding
  only an argon2 password hash. See `src-tauri/src/db/product_owner.rs`.
- **Entry point**: 7 clicks/taps within 3 seconds on the store icon on the
  login screen, or on the business name in the top bar once signed in (see
  `useSecretTapTrigger`). No visible affordance hints at this.
- **Session**: separate from the client staff session, memory-only, and
  expires after 10 minutes idle (`src-tauri/src/product_owner_session.rs`).

## Recovering a forgotten credential on a given install

There is **no in-app reset path** for this account, by design — a reset
flow reachable by any client role would defeat the entire point of it being
vendor-only. If you forget the credential on a specific install, this is a
manual, vendor-side procedure requiring access to that machine (in person,
remote desktop, or an equivalent support channel a client grants you):

1. Locate the app's SQLite database. Tauri's `app_data_dir()` for this app:
   - Windows: `%APPDATA%\com.applr.pos\pos.db`
   - macOS: `~/Library/Application Support/com.applr.pos/pos.db`
   - Linux: `~/.local/share/com.applr.pos/pos.db`
   - Android: the app's private data directory (requires `adb` + a
     debuggable build, or asking the client to reinstall — see below).
2. **Close the app first** (SQLite with an open connection can be locked;
   don't edit the live file under a running process).
3. Clear the stored credential so the app treats the install as "no
   credential set yet" — the hidden entry point then shows the setup form
   again on next tap-trigger:
   ```bash
   sqlite3 pos.db "DELETE FROM product_owner_account WHERE id = 1;"
   ```
4. Reopen the app, trigger the hidden entry point, and set a new
   credential. Any module locks you'd previously set are untouched by
   this — they live in `enabled_modules`, not in this table.

If you don't have any remote/file access to the machine at all (e.g. an
Android install with no debug bridge available), there is no recovery
short of the client reinstalling the app, which resets the local database
entirely (including their own config/data) — communicate that trade-off to
them before suggesting it.

## Honest limitation

This is local, offline software with no server-side verification. Hiding
the entry point and hashing the credential raises the bar against a
client's own staff poking around the UI — it does **not** protect against
someone with direct filesystem access to the machine (inspecting `pos.db`
directly bypasses the UI's discovery problem entirely) or a technically
capable person willing to reverse-engineer the installed binary. Don't
represent this to clients (or to yourself) as unbreakable; it isn't meant
to be.
