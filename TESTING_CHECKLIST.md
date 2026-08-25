# Manual Testing Checklist

This is Phase 13's manual companion to `cargo test` (141 automated tests as of
this phase — money math, stock edge cases, transaction rollback, and the
permission matrix are already covered there; this file is for the things a
unit test can't reach: killing the process, real module-toggle combinations,
missing hardware, and wall-clock date boundaries).

Run through this after any change that touches `db::sales`, `db::tables`,
`db::salary`, the module-config system, or receipt/report generation. Check
each box against a real running app (`npm run tauri dev`), not the dev server
alone (`npm run dev` can't reach `invoke()`).

## 1. Data integrity: killing the app mid-sale

The goal is confirming `Db::with_transaction` actually protects every
multi-step write end to end, not just in the unit tests that construct a
`Transaction` directly.

- [ ] Start a sale: add 2–3 items to the cart, apply a discount, do **not**
      click Complete Sale yet.
- [ ] Force-quit the app (`kill -9` the process, or use the OS task manager —
      not a graceful window close) while `billing_create_sale` would be
      mid-flight. In practice this means: click Complete Sale, and kill the
      process within the same second.
- [ ] Relaunch. Confirm:
  - [ ] Stock levels for every item in that cart match what they were
        **before** the kill (Inventory screen) — not partially decremented.
  - [ ] No new row exists in `sales`/`sale_items` for that attempt (check via
        Reports, or `app_db_tables`/a SQLite browser against
        `pos.db` in the app's data directory).
  - [ ] The app boots normally — a half-written WAL file must not corrupt the
        database on next open (SQLite's WAL mode is designed for exactly
        this; confirm it holds in practice, not just in theory).
- [ ] Repeat with a **table-linked** sale (Tables module on, sale attached to
      an occupied table): kill mid-sale, relaunch, confirm the table is
      still `occupied` with its cart still parked — not silently freed with
      no sale to show for it, and not stuck in a half-updated state.
- [ ] Repeat for a **salary payment**: on the Salary screen, click Record
      Payment, and kill the process right after submitting. Relaunch and
      confirm the payment is either fully recorded (amount + date both
      present) or not recorded at all — never a paid amount bumped with no
      `paid_date`, or vice versa.

If any of these show a partial write, it's a transaction-audit regression —
see the "Transaction audit" section of this phase's summary for the specific
functions that must own an explicit `Db::with_transaction` around every
multi-statement write.

## 2. Module combinations

The module system (Phase 1) and the Phase 11 permission matrix both key off
`enabled_modules` and role — this checks the two systems don't contradict
each other or leave a dangling reference when a module goes from on to off
*while data from it already exists*.

For each combination below: toggle it in Settings (as Owner), reload the
app, and confirm the change takes effect with **zero code changes** —
navigating directly to the now-disabled module's URL (e.g. typing `#/salary`
in the address bar) must redirect away, not show a broken/empty screen.

- [ ] **Retail default**: Tables, Attendance off; Inventory, Billing,
      Reports, Expenses, Salary, Dashboard on.
  - [ ] Billing screen shows no table selector at all (not a disabled one).
  - [ ] Dashboard's snapshot cards render without a low-stock card only if
        there truly are none low — and without an expenses card if Expenses
        is the one you toggled off instead, not a `$0.00` placeholder.
- [ ] **Restaurant default**: everything on.
  - [ ] Billing shows the table selector; seating a table from the floor
        view opens Billing pre-linked to it.
- [ ] **Everything off except core Billing**: turn off Inventory, Tables,
      Reports, Attendance, Expenses, Salary, Dashboard one at a time (or all
      at once) as Owner.
  - [ ] Billing still works standalone — search, add to cart, complete a
        sale, get a receipt — with none of the disabled modules' UI
        anywhere (sidebar, quick-links, table selector).
  - [ ] Confirm Billing itself **cannot** be turned off (no toggle exists /
        toggling it is rejected) — it's the one always-core module.
- [ ] **Toggle Expenses off after it already has data**: log a few expenses,
      turn the module off, open Dashboard.
  - [ ] Net profit no longer subtracts expenses at all (not `$0` expenses —
        the field should be absent from the calculation), even though the
        old expense rows are still sitting in the database. Re-enable the
        module and confirm the old data reappears unchanged.
- [ ] **Cashier role sanity check** (re-run of the Phase 11 requirement,
      worth re-verifying whenever module wiring changes): log in as a
      Cashier account with every module enabled at the installation level.
      Confirm Reports, Expenses, Salary, Settings, and User Management are
      unreachable both from the sidebar and by typing their URL directly.

## 3. Printer failures — must never crash the app

`printer::escpos::send_to_printer` returns a proper `Result` for every
failure mode on every platform (see its module doc comment and
`PrinterError`) — this section confirms that holds in practice, not just in
`cargo test`. The historical bug this guards against: an earlier build
called into raw USB (`rusb`/libusb) unconditionally on Android too, and
libusb was never designed for Android's USB-permission model — the OS
killed the whole app with a native fault, not a catchable Rust error. USB is
now `cfg`'d out of Android builds entirely (see `Cargo.toml`), and Android's
own transport (Bluetooth, `printer::android_bt`) is hand-written JNI, which
carries its own version of this risk if a method signature is ever wrong —
treat any change to `android_bt.rs` as needing this whole section re-run on
a real device, not just a successful `cargo build`. Three specific pitfalls
already hit and fixed once during that module's development, worth knowing
about before touching it again:

- Getting a `JavaVM`/`Context` without Tauri's cooperation is genuinely
  hard on this platform — `ndk_context::android_context()` panics
  ("android context was not initialized") because nothing in this Tauri
  version's Android runtime actually calls
  `ndk_context::initialize_android_context`, and `JNI_GetCreatedJavaVMs`
  (fine on desktop JVMs) doesn't even link on Android — `UnsatisfiedLinkError:
  dlopen failed: cannot locate symbol`, which fails the *entire app* to
  load, every launch, not just this feature. What actually works: a
  `#[no_mangle] extern "system" fn JNI_OnLoad` (any native library gets this
  called automatically, with a live `JavaVM*`, per the JNI spec itself) —
  see `android_bt.rs`'s module doc comment.
- `ContextCompat.checkSelfPermission` (AndroidX) threw an opaque "Java
  exception was thrown" in a *release* build specifically, because R8 had
  stripped the class as apparently-unused — nothing in the Kotlin/Java side
  references it, only JNI reflection does, invisible to R8's reachability
  analysis. Fixed by calling the plain framework `Context.checkSelfPermission`
  instead (present since API 23, this app's `minSdk`) — never stripped,
  since it's not part of the app's own dex. Any future JNI call to an
  AndroidX/support-library class (as opposed to a plain `android.*`
  framework one) should be treated as suspect for the same reason and
  tested against a *release* build, not just debug (R8 minification is
  release-only).
- Test on a **release** build (`tauri android build`, not `tauri android
  dev`) specifically for anything touching this file — a debug build skips
  R8 entirely, so it can't reproduce the class-stripping failure mode above.

**macOS/Linux, no USB printer attached** (true for most dev machines, which
makes this an easy one to actually run):

- [ ] Complete a sale. On the receipt screen, click "Print (thermal)".
  - [ ] A clear, human-readable error appears in the modal (not a frozen
        button, not a console-only error) — should read something like "no
        thermal printer was found" rather than a raw Rust error or a blank
        failure.
  - [ ] The PDF download button on the same screen still works regardless —
        confirm the thermal failure didn't affect the receipt PDF path at
        all (they're independent).
- [ ] Confirm the sale itself is **not** rolled back or affected by the
      printer failure — printing is a post-sale action, not part of the
      `billing_create_sale` transaction, so a failed print must never look
      like a failed sale.

**Windows, no printer selected yet** (the default state for every install
until Settings' "Select printer" step has been used once — Windows,
unlike macOS/Linux, never auto-detects; see `printer::windows_spool`'s
module doc comment for why raw USB scanning doesn't work once a printer
driver is installed):

- [ ] Fresh install (or an install that's never had a printer selected).
      Complete a sale, click "Print (thermal)".
  - [ ] A clear message appears ("No printer is set up yet…") and the PDF
        button still works — no crash, no raw Rust error.
- [ ] Go to Settings → Printer. Confirm the installed printer (the same one
      visible in Windows' own "Devices and Printers"/"Printers & scanners")
      appears in the list by name, and selecting it is reflected as
      "Selected: <name>".
- [ ] Complete a sale, click "Print (thermal)" (or let it fire
      automatically): confirm a real receipt prints through that printer —
      table borders intact, correct cut spacing, no stray characters at the
      top (the wake-padding fix), logo present if one is configured. This is
      the actual regression test for the bug this module fixed: it must
      print via the RAW spooler path, not silently succeed some other way.
- [ ] Uninstall/disconnect that printer in Windows (or select a printer
      name that no longer exists) and print again:
  - [ ] The app does not crash.
  - [ ] A clear message appears and the PDF button still works.

**Android, no printer selected yet** (the default state for every install
until Settings' "Select printer" step has been used once):

- [ ] Fresh install (or an install that's never had a printer selected).
      Complete a sale, tap "Print (thermal)".
  - [ ] The app does **not** crash or show the OS "app closed because of a
        bug" dialog.
  - [ ] A clear in-app message appears ("No printer is set up yet…") and the
        PDF button still works.

**Android, Bluetooth permission denied:**

- [ ] Go to Settings → Printer. If `BLUETOOTH_CONNECT` is already granted
      from a previous run, revoke it first (Android system Settings → Apps →
      POS → Permissions → Nearby devices → Don't allow), then reopen
      Settings → Printer in the app.
  - [ ] The screen shows "Grant Bluetooth permission" rather than an empty
        or broken device list.
  - [ ] Tapping it shows the real OS permission dialog.
  - [ ] Denying it leaves the app on a clear message, not stuck loading and
        not crashed.
- [ ] With permission still denied, complete a sale and tap "Print
      (thermal)" directly (without visiting Settings first) — same
      requirement: clear message, PDF fallback works, no crash.

**Android, printer selected but unreachable** (paired once, then turned off
or out of range — the "disconnected mid-print" case):

- [ ] In Settings → Printer, select a paired Bluetooth device (any paired
      device works for this test, even one that isn't actually a printer —
      the point is exercising the connect-failure path, not a successful
      print).
- [ ] Turn that device off (or walk out of Bluetooth range).
- [ ] Complete a sale, tap "Print (thermal)".
  - [ ] The app does not crash.
  - [ ] A clear message appears (e.g. "couldn't connect — check the printer
        is on and in range") and the PDF button still works.
- [ ] Repeat with a **real thermal printer**, paired and in range: confirm
      an actual receipt prints, with the logo (if one is configured),
      itemized lines, and totals all legible — this is the one case none of
      the above can substitute for.

## 4. Date-range edge cases

Exercises `reports::validate_range`/`get_sales_over_time` and
`salary::get_monthly_summary`'s calendar-month math on real wall-clock dates,
not just the fixed test dates in `cargo test`.

- [ ] **Reports spanning a month boundary** (e.g. Jan 28 – Feb 3): confirm
      the daily chart has one bar per calendar day across the boundary with
      no gap or duplicate around the 1st, and the summary totals match a
      manual sum of the visible days.
- [ ] **Reports spanning a year boundary** (e.g. Dec 29 – Jan 4): same check
      — this is the one most likely to break a `substr(created_at, 1, 7)`-
      style month grouping if one ever gets introduced.
- [ ] **Single-day range** (start == end): summary and chart both show
      exactly that one day, not an empty result.
- [ ] **Attendance/Salary monthly summary for the current, in-progress
      month**: confirm `daysAbsent` counts only days up to *today*, not the
      full calendar month — an employee shouldn't show as "absent" for days
      that haven't happened yet.
- [ ] **Attendance/Salary monthly summary for a fully past month**: confirm
      it uses the whole month (through its actual last day — including
      correctly handling a 28/29/30/31-day month, and December → January
      rollover) rather than capping at 30.
- [ ] **A custom range with start after end** typed into the date pickers:
      confirm the UI either prevents it or shows the "start date must not be
      after the end date" error clearly — not a blank/broken chart.

## 5. Spot-check while you're in there

Not exhaustive, but worth a glance alongside the above:

- [ ] Kill the app mid-way through checking an employee **in** (Attendance).
      Relaunch: either a full check-in row exists or none does — never a row
      with a `work_date` but garbage/partial timestamps.
- [ ] With a low-spec or throttled machine if one's available: confirm the
      app still opens in a couple of seconds and Billing stays responsive —
      per `projectGoal.md`'s "instant on old shop PCs" success criterion.
- [ ] Re-run the Phase 6 table loop once more end to end (seat → bill → auto-
      free) after any change near `db::tables` or `db::sales`, since it's
      the one flow that spans both modules' transactions in a single call.
