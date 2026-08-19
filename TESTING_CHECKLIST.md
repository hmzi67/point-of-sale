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

## 3. No printer connected

`printer::escpos::send_to_printer` always returns "not configured" until
real hardware is wired up — this just confirms that failure path is graceful
end to end, on a machine with no thermal printer attached (true for most dev
machines, which makes this an easy one to actually run).

- [ ] Complete a sale. On the receipt screen, click "Print (thermal)".
  - [ ] A clear, human-readable error appears in the modal (not a frozen
        button, not a console-only error) — should read something like "not
        configured" rather than a raw Rust error or a blank failure.
  - [ ] The PDF download button on the same screen still works regardless —
        confirm the thermal failure didn't affect the receipt PDF path at
        all (they're independent).
- [ ] Confirm the sale itself is **not** rolled back or affected by the
      printer failure — printing is a post-sale action, not part of the
      `billing_create_sale` transaction, so a failed print must never look
      like a failed sale.

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
