# Diwan

A white-label, offline-first point of sale system for small retail and restaurant
businesses, built with Tauri 2 (React + Rust). One codebase, configured per
client — modules are toggled on or off per installation rather than forked into
separate builds.

See [`projectGoal.md`](./projectGoal.md) for the product goals and
[`buildPlan.md`](./buildPlan.md) for the phase-by-phase build history. For
day-to-day use of the app itself (non-technical), see
[`ADMIN_GUIDE.md`](./ADMIN_GUIDE.md).

## What's included

- **Billing / POS** — core, always-on. Barcode-scan-friendly search, category
  browsing, per-item notes, discounts, tax, dine-in table support, PDF/ESC-POS
  receipts.
- **Inventory** — stock, categories, low-stock alerts, product photos, CSV bulk
  import.
- **Tables** — restaurant floor view, park-and-resume orders (toggleable).
- **Reports** — sales summaries, top items, PDF/CSV export.
- **Attendance** — staff check-in/out, monthly summaries (toggleable).
- **Expenses** — categorized expense logging (toggleable).
- **Salary** — attendance-driven salary calculation and payment tracking
  (toggleable).
- **Dashboard** — daily snapshot and trend, aggregated from whatever modules
  are enabled.
- **Settings & Users** — per-installation module toggles, PIN-based accounts,
  role-based access (Owner / Admin / Cashier).
- A first-run **setup wizard** (business info → suggested module defaults →
  module toggles) so a new client is configured, not coded, into existence.

Everything runs fully offline against a local SQLite database — no server, no
account, no internet dependency for any core task.

## Stack

- **Frontend**: React 19 + React Router 7, Zustand for state, Tailwind CSS v4,
  `lucide-react` icons, `recharts` charts, `jspdf` for receipt/report PDFs.
- **Backend**: Rust with `rusqlite` (bundled SQLite), `chrono`, `argon2` for PIN
  hashing, exposed to the frontend as typed Tauri commands.

See [`CLAUDE.md`](./CLAUDE.md) for the full architecture reference (module
system, IPC conventions, database invariants, permission model) — that's the
canonical doc for anyone (human or AI) working on this codebase.

## System requirements

- **Windows** — 10 or later (64-bit). WebView2 Runtime, which Tauri relies
  on for the app UI, dropped support for Windows 7/8/8.1 in January 2023
  (v109 was the last runtime build to support them, and Microsoft no longer
  distributes it through any official channel); an NSIS preinstall check
  (`src-tauri/windows/hooks.nsh`) stops with a plain-language message
  before touching anything if the target machine is below Windows 10. The
  installer uses the small `downloadBootstrapper` webview install mode
  (`tauri.conf.json`), so a Windows 10+ target machine needs internet
  access the first time it installs Diwan, to fetch WebView2 if it isn't
  already present — see `DEPLOYMENT.md`'s "Windows 7 is not supported"
  section for the size/offline trade-off this implies.
- **macOS** — 10.15 (Catalina) or later, per Tauri/WKWebView's own floor.
- **Android** — see `src-tauri/gen/android` / the Android build job in
  `.github/workflows/release.yml` for the target/min SDK once that build is
  re-enabled; not yet finalized.

## Getting started

```bash
npm install
npm run tauri dev      # run the desktop app (Vite on :1420, then cargo)
```

First run seeds a demo shop (debug builds only) and an `Owner` account with PIN
`1234` — sign in with that and change it via the Users screen. A fresh
production build instead launches straight into the first-run setup wizard.

### Other commands

```bash
npm run tauri build    # production installer (.app/.dmg, .msi, .AppImage, …)
npm run dev             # Vite only, in-browser — invoke() calls will fail here
npm run build            # tsc --noEmit + vite build
```

Rust-only checks, run from `src-tauri/`:

```bash
cargo check
cargo fmt
cargo clippy
cargo test              # single test: cargo test <name_substring>
```

There's no frontend linter or test runner configured; the safety net on that
side is `tsc --noEmit` plus manual testing against
[`TESTING_CHECKLIST.md`](./TESTING_CHECKLIST.md).

## Layout

```
src/
  components/<module>/   per-module components (layout/, billing/, inventory/, …)
  pages/                 one screen per module
  hooks/, store/         shared hooks and zustand stores
  services/               typed wrappers over Tauri commands — all IPC goes through here
  types/, utils/          shared domain types, formatting, nav catalogue
src-tauri/src/
  commands.rs             every #[tauri::command]
  session.rs              server-side session — the actual source of truth for role checks
  db/schema.sql           the whole schema (ER summary in its header comment)
  db/schema.rs             migration runner + first-run seeding
  db/{config,items,sales,tables,attendance,expenses,salary,dashboard,csv_import}.rs
  printer/escpos.rs        ESC/POS byte-building; hardware send is a documented stub
```
