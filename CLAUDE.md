# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Tauri 2 desktop app (`pos` — point of sale), white-label by design: one codebase configured per client, with modules toggled on/off rather than forked. Read `projectGoal.md` for the "why" and `buildPlan.md` for the phase-by-phase "how" — every change should fit somewhere in that plan.

Phases 0–4 are complete: the app shell, the module-configuration system, PIN auth, the full database schema with demo seed data, Inventory (CRUD, search/filter, low-stock flagging, product photos), and Billing/POS (search-and-scan cart, atomic sale transaction, discount/tax, dine-in table park-and-resume, PDF receipt, ESC/POS stub). The remaining feature modules are still placeholder pages.

Stack:
- Frontend: React 19 + React Router 7, `zustand` for state, Tailwind CSS v4 (via `@tailwindcss/vite`), `lucide-react` icons, `recharts` charts, `jspdf` + `jspdf-autotable` for receipt/report PDFs.
- Backend: Rust with `rusqlite` (bundled SQLite) for local persistence, `chrono` for timestamps, exposed to the frontend as `#[tauri::command]`s.

## Commands

```bash
npm run tauri dev      # run the desktop app (spawns vite on :1420, then cargo)
npm run tauri build    # production bundle
npm run dev            # vite only, browser — `invoke()` calls will fail here
npm run build          # tsc --noEmit typecheck + vite build
```

Rust-only checks run from `src-tauri/`: `cargo check`, `cargo fmt`, `cargo clippy`, `cargo test` (single test: `cargo test core_modules_cannot_be_disabled`).

There is no frontend linter or test runner configured. If adding one, wire the script into `package.json` and note the single-test invocation here.

## Layout of the code

```
src/
  components/<module>/   per-module components (layout/, ui/, billing/, inventory/, …)
  pages/                 one screen per module, exported from pages/index.ts
  hooks/                 shared React hooks
  services/              typed wrappers over Tauri commands — all IPC goes through here
  store/                 zustand stores
  types/                 shared domain types
  utils/                 formatting, nav catalogue
src-tauri/src/
  commands.rs            every #[tauri::command]
  db/schema.sql          the whole schema (ER summary in its header comment)
  db/schema.rs           migration runner + first-run seeding, plus test_conn()
  db/seed.rs             demo data (debug builds / POS_SEED_DEMO=1 only)
  db/{config,modules,users,items,sales}.rs   per-area queries
  images.rs              product-photo file storage (not SQLite — see below)
  printer/escpos.rs       ESC/POS byte-building (tested); send_to_printer() is the hardware stub
src/utils/receiptPdf.ts    jsPDF receipt builder — the actual default/working receipt path
src/utils/billingTotals.ts subtotal/discount/tax/total math, shared by the cart UI and the sale submission
```

## Architecture notes

- **IPC boundary**: all SQLite access belongs in Rust. Add commands in `src-tauri/src/commands.rs`, register them in the `generate_handler![]` list in `lib.rs`, and expose them to the frontend as a function in `src/services/`. Arg names are camelCase on the JS side, snake_case in Rust.
- **Never call `invoke()` from a component.** `src/services/tauriClient.ts` is the only module that imports `@tauri-apps/api`; everything else goes through its `call()` / `callSafe()` helpers, which wrap failures in `TauriCommandError`.
- **Command naming**: `<module>_<action>` — `inventory_add_item`, `billing_create_sale`, `config_get_enabled_modules`.
- **Database handle**: a single `Db` (mutex-guarded `rusqlite::Connection`, WAL, foreign keys on) is opened in `lib.rs` `setup()` at `app_data_dir()/pos.db` and stored as managed state — take it in a command as `db: State<'_, Db>`.
- **Modules are dynamic**: what a client sees comes from `enabled_modules` in SQLite, never from code. `src/utils/navigation.ts` maps a `ModuleKey` to a route and icon (presentation only); labels and ordering come from the `modules` table. To add a module: add a row to `MODULE_CATALOGUE` in `db/schema.rs`, a key to `ModuleKey`, an entry to `MODULE_NAV`, and a guarded `<Route>`.
- **`useModules()` is the single source of truth** for "what can the current user see?" — it intersects enabled modules with the role. The sidebar and the `ModuleRoute` guard both read it, so they cannot disagree. Billing is core: it can never be disabled, which is what guarantees the guard's fallback path always exists.
- **Auth**: local accounts with argon2-hashed PINs (4–6 digits). The session lives in `useAuthStore` in memory only — no tokens, no persistence; closing the app ends the shift. First run seeds an `Owner` account with PIN `1234`.
- **Roles**: `src/utils/permissions.ts` is the only place role rules live. Owner/admin see everything enabled plus Settings; cashiers get Billing and read-only Inventory. Role checks stack on top of module config, never replace it.
- **Settings is not a module** — it is the screen that configures them, so it is admin-only and always present.
- **Money is INTEGER minor units** (paisa/cents) in every column, suffixed `_minor`. Never floats — a till must not accumulate rounding drift. Convert only at the print/display boundary. Percentages (tax) stay REAL, but any amount derived from one is rounded straight back to whole minor units.
- **Migrations are additive-only**: `schema.sql` is all `CREATE ... IF NOT EXISTS` and is replayed on every launch, so it creates what is missing and rewrites nothing. `apply()` logs which tables were missing, verifies `EXPECTED_TABLES` afterwards, and bumps `PRAGMA user_version`. A change that must *modify* an existing table needs a numbered step guarded on `user_version` instead — never edit an existing `CREATE TABLE` in place once clients have data.
- **Historic data is immutable**: `sale_items.price_at_sale_minor` is copied at checkout, and deleting an item that appears on any sale is refused (`ON DELETE RESTRICT`) — retire it with `is_active = 0`.
- **Seeding must never clobber onboarding**: demo data only runs when the operational tables are empty, and it fills in config only where the client has not set a value.
- **Delete is soft where history exists**: `inventory_delete_item` hard-deletes an item that has never appeared on a sale; one that has is archived (`is_active = 0`) instead — the command returns `"deleted" | "archived"` so the UI can tell the cashier which happened. `inventory_get_items` excludes archived items unless `includeInactive` is passed.
- **Item add/edit is a full-row replace**, not a partial patch (`ItemInput` has every editable field required) — this is what lets clearing a category (or a photo) back to unset work unambiguously, unlike `AppConfigUpdate`'s `COALESCE`-based partial patch.
- **Product photos live on disk, never in SQLite**: `images.rs` writes each upload as its own file under `app_data_dir()/product-images/`, self-named (never the client's original filename — avoids path-traversal and collisions) and capped at `MAX_IMAGE_BYTES` (5 MB). `items.image_path` stores only that generated filename, not a path, so it survives a reinstall or an app-data-dir move. The frontend never sees raw bytes over IPC except once, on upload/read: `inventory_upload_image` takes base64 (the browser's `FileReader` already produced it, so no filesystem-access plugin/capability was needed) and returns the filename; `inventory_get_image` takes a filename and returns a `data:` URL ready for `<img src>`. `src/store/inventoryStore.ts` caches fetched data URLs by filename (`imageCache`/`ensureImage`) so a table of thumbnails fetches each image once.
- **A replaced or removed photo is deleted from disk** by the `inventory_update_item`/`inventory_delete_item` *commands* (not the `db::items` functions, which stay pure-SQLite) — they diff the old `image_path` against the new one after a successful write and call `images::delete_image`. A photo is only removed when its item is truly deleted, never when archived (an archived item can still be looked up). Known gap: if a user picks a photo, then picks a different one before saving (never submitting the form), the first upload is orphaned on disk — bounded, low-severity, not yet cleaned up.
- **`items.image_path` was added after the table's first release**: `schema.sql`'s `CREATE TABLE IF NOT EXISTS` can't add a column to an already-existing table, so `db/schema.rs`'s `ADDED_COLUMNS` runs a guarded `ALTER TABLE ... ADD COLUMN` for it (and is the template for any future added-later column) — see the module doc comment for why this is still additive-only. `table_orders.cart_json` (below) was added the same way.
- **A sale is one SQLite transaction** (`Db::with_transaction`, used by `billing_create_sale`): `db::sales::create_sale` re-prices every cart line against the live `items` row (never trusts a client-sent price), checks stock, inserts `sales` + `sale_items`, decrements stock, and — if a table was given — closes that table's parked order and frees it. Any `?` inside bails out before the caller's `tx.commit()`, so a failure at any step leaves nothing behind; `db::sales::tests::insufficient_stock_rolls_back_the_whole_sale` proves this directly.
- **Billing search re-prices nothing** — `items::search_items` is read-only, ranks an exact barcode match first (so a scanner's Enter reliably adds "the top result"), and is a separate query from `inventory_get_items`/`list_items` (different ordering needs).
- **Dine-in tables (`tables` module)**: "Save to table" parks the cart as JSON on `table_orders.cart_json` (a deliberate Phase 4 stand-in for real draft line items — see `db/sales.rs`'s module doc; Phase 6, full Table Management, is expected to replace it) and marks the table `occupied`. Selecting an `occupied` table with a parked order shows "Load parked order" to resume it. Completing a sale with `table_id` set closes that table's open order (`status = 'billed'`) and frees the table (`status = 'free'`) inside the same transaction. `BillingPage` reads `tables` module enablement straight from `useModules().modules` (not `visibleModules`, which is role-filtered) and renders none of `TableSelector` when it's off — there is no "disabled" state to design for inside that component.
- **Money the client computes vs. the server re-derives**: `discountMinor` and `taxMinor` are computed client-side (`utils/billingTotals.ts`, mirroring the Rust seed data's `round(taxable * percent / 100)` formula) and sent as-is — trusted the same way `AppConfig.taxPercent` is. Only *price × qty* is re-derived server-side, because that is the one value with real tamper/staleness risk (a cart built minutes ago against a price that has since changed).
- **PDF is the real default receipt, not a placeholder**: `utils/receiptPdf.ts` (jsPDF + `jspdf-autotable`) builds an actual 80mm-thermal-shaped receipt and is dynamically `import()`-ed from `ReceiptModal` so its ~140KB chunk never loads on the billing screen itself. `printer/escpos.rs` is the real stub half — `build_receipt_bytes` is complete and tested, `send_to_printer` always returns `NotConfigured` until real hardware (USB/serial/network) is wired in; treat that as "fall back to PDF," never as a reason to fail a sale that's already committed.
- **Cart state is keyed by item id, not an array** (`useBillingStore.cart: Record<number, CartEntry>` + a separate `cartOrder: number[]` for row order): each `CartRow` subscribes to only `state.cart[itemId]`, so editing one line's quantity re-renders that row and the totals footer, not the rest of the cart. Search input/results live in local component state in `ItemSearchBar`, not the store, for the same reason in the other direction — typing there never touches cart state.
- **Permissions**: any Tauri plugin capability the frontend uses must be listed in `src-tauri/capabilities/default.json` or the call is denied at runtime.
- **Fixed dev port**: Vite is pinned to 1420 with `strictPort` because `tauri.conf.json` `devUrl` points there. Don't change one without the other.
- `withGlobalTauri: true` and `csp: null` are template defaults; tighten the CSP before shipping.
- Routing uses `HashRouter`: Tauri serves the bundle over a custom protocol in production, where a reload on a deep path would 404 under `BrowserRouter`.
- `productName`/window title are `POS` and the identifier is `com.applr.pos`. The Cargo package is still named `tauri-app`, so the dev binary is `target/debug/tauri-app`.
