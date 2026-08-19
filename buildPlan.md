# POS System — Full Build Plan (Start to End)

This plan takes the project from empty repo to a shippable, multi-client, module-configurable POS system across Desktop (Tauri) and Android.

Reference: see `PROJECT_GOALS.md` for the "why" behind these decisions — this file is the "how" and "in what order."

---

## Phase 0 — Foundations (Setup & Architecture Decisions)

**Goal:** Environment ready, architecture decided before writing feature code.

1. Project scaffolding (done) — Tauri + React + TypeScript + TailwindCSS
2. Install core dependencies: Zustand (state), Recharts (charts), jsPDF (PDF export), Lucide (icons)
3. Set up SQLite in Rust (`rusqlite` with `bundled` feature)
4. Decide and document:
   - Folder structure (frontend `src/`, backend `src-tauri/src/`)
   - Naming conventions for Tauri commands (e.g. `inventory_add_item`, `billing_create_sale`)
   - How frontend calls backend (`invoke()` wrapper service layer — never call `invoke()` directly from components)
5. Set up Git repo + `.gitignore` (node_modules, target, dist)
6. Decide on a base UI layout: sidebar nav (dynamic, driven by module config) + top bar + main content area

**Deliverable:** Empty app that runs, with a sidebar shell and routing in place (React Router).

---

## Phase 1 — Core Architecture: Config & Module System

**Why first:** Every other module depends on this. Retrofitting later is expensive (per project goals).

1. **Database: `app_config` / `clients` table**
   - Stores: business name, business type (retail/restaurant/etc.), logo path, currency, tax %, receipt footer text
2. **Database: `modules` table**
   - Fixed list of all possible modules (inventory, billing, reports, tables, attendance, expenses, salary, dashboard)
   - Each module: `id`, `name`, `key`, `is_core` (billing = true, rest = false)
3. **Database: `enabled_modules` table**
   - Links `client/installation` → `module` → `is_enabled` (bool), plus **per-platform flag** (`desktop_enabled`, `android_enabled`)
4. **Backend (Rust):**
   - Tauri commands: `get_enabled_modules()`, `toggle_module(module_key, platform, enabled)`
5. **Frontend (React):**
   - `useModules()` hook — fetches enabled modules on app load, stores in Zustand
   - Sidebar renders links dynamically from this list — no hardcoded nav items
   - Route guard: navigating directly to a disabled module's URL redirects to dashboard
6. **Simple Settings screen (admin-only):**
   - Toggle switches per module (desktop view) — this becomes the "onboard a new client" screen

**Deliverable:** Sidebar that changes when you toggle a module off/on, with zero code changes — proves the core config-driven architecture works before building real features on top of it.

---

## Phase 2 — Database Schema (Full Design)

**Goal:** Design all core tables up front so later modules don't require breaking schema changes.

Core tables to design in this phase (SQL written, migrations set up):

| Table | Purpose |
|---|---|
| `users` | Login accounts — owner/admin/cashier/staff roles |
| `roles` / `permissions` | What each role can see/do (ties into module visibility too) |
| `categories` | Item categories for inventory |
| `items` | Inventory items — name, SKU/barcode, price, cost, stock qty, category, low-stock threshold |
| `sales` | One row per completed sale/bill — total, discount, tax, payment method, timestamp, cashier |
| `sale_items` | Line items per sale — item, qty, price at time of sale |
| `tables` | Restaurant tables — number/name, status (free/occupied/reserved) |
| `table_orders` | Active order linked to a table, linked to a draft sale |
| `employees` | Staff records — name, role, contact, salary base |
| `attendance` | Daily check-in/check-out log per employee |
| `expenses` | Date, category, amount, note |
| `salary_payments` | Employee, month, calculated amount, paid amount, paid date |
| `app_config`, `modules`, `enabled_modules` | (from Phase 1) |

**Approach:**
1. Draw ER diagram (relationships: sales → sale_items → items, attendance → employees → salary_payments)
2. Write raw SQL schema file (`schema.sql`)
3. Set up a simple migration runner in Rust (even a basic "run schema.sql if tables don't exist" is fine for v1)
4. Seed script for demo/test data (a few items, one employee, sample sales) — makes UI development much faster later

**Deliverable:** Complete schema file + migration runner + seed data script.

---

## Phase 3 — Inventory Management Module

**Why this module first (after config/schema):** Simplest CRUD, proves the full stack (DB ↔ Rust command ↔ frontend) works end-to-end. Billing depends on inventory data existing.

1. Backend commands: `get_items`, `add_item`, `update_item`, `delete_item`, `get_categories`, `add_category`
2. Frontend:
   - Item list view (table, searchable, filter by category)
   - Add/Edit item form (name, SKU/barcode, price, cost, stock, category, low-stock threshold)
   - Low-stock badge/alert on dashboard-relevant items
3. Barcode field ready for scanner input (scanners act as keyboard input — no special hardware integration needed at this stage)

**Deliverable:** Full inventory CRUD working, stock levels visible and editable.

---

## Phase 4 — Billing / POS Module (Core Feature)

**Why now:** This is the most-used screen daily — gets the most polish time. Depends on Inventory being functional.

1. Backend commands: `search_items(query)`, `create_sale(cart, discount, payment_method)`, `get_sale(id)`
2. Frontend — POS screen:
   - Fast item search bar (name or barcode scan) with keyboard-first UX (Enter to add to cart)
   - Cart panel: item, qty (editable), price, line total, remove button
   - Discount field (flat or %)
   - Tax auto-calculated from config
   - Payment method selector (cash/card/other)
   - "Complete Sale" → writes `sales` + `sale_items`, decrements stock, opens receipt
3. Receipt printing:
   - ESC/POS integration (Rust crate, e.g. `escpos-rs` or raw byte commands over USB/serial)
   - Fallback: "print to PDF" if no thermal printer connected (important for testing without hardware)
4. Restaurant mode (if Table Management enabled): billing screen can attach sale to a table instead of completing immediately

**Deliverable:** A cashier can search/scan an item, build a cart, apply discount, complete a sale, and get a printed (or PDF) receipt — stock updates automatically.

---

## Phase 5 — Daily Reports Module

**Depends on:** Billing data existing (Phase 4).

1. Backend commands: `get_sales_summary(date_range)`, `get_top_items(date_range)`, `export_report(format)`
2. Frontend:
   - Date range picker (today / this week / this month / custom)
   - Summary cards: total sales, total transactions, avg. sale value
   - Chart: sales over time (Recharts line/bar chart)
   - Table: top-selling items
   - Export buttons: PDF (jsPDF) and Excel/CSV

**Deliverable:** Owner can pull a report for any date range and export it.

---

## Phase 6 — Table Management Module (Restaurant Clients Only)

**Toggle-gated:** Only relevant/visible if client is restaurant-type and module enabled.

1. Backend commands: `get_tables`, `update_table_status`, `assign_order_to_table`, `clear_table`
2. Frontend:
   - Visual table grid/floor view — color-coded by status (free/occupied/reserved)
   - Tap a table → opens billing screen pre-linked to that table
   - "Clear table" after payment completes

**Deliverable:** Restaurant staff can manage table status and route orders through billing per-table.

---

## Phase 7 — Employee Attendance Module

1. Backend commands: `check_in(employee_id)`, `check_out(employee_id)`, `get_attendance(employee_id, date_range)`, `get_monthly_summary`
2. Frontend:
   - Simple check-in/check-out screen (button per employee, or PIN-based clock-in)
   - Attendance log table (date, in-time, out-time, hours worked)
   - Monthly summary view (days present/absent per employee) — feeds into Salary module

**Deliverable:** Daily attendance logging + monthly summary ready to feed salary calculation.

---

## Phase 8 — Expense Tracker Module

1. Backend commands: `add_expense`, `get_expenses(date_range)`, `get_expense_categories`
2. Frontend:
   - Quick-add expense form (amount, category, date, note)
   - Expense list, filterable by category/date
   - Category-wise totals (feeds into dashboard profit calc)

**Deliverable:** Daily expenses logged and categorized.

---

## Phase 9 — Salary Management Module

**Depends on:** Attendance (Phase 7) for auto-calculation.

1. Backend commands: `calculate_salary(employee_id, month)` (base salary ÷ working days × days present, or fixed — configurable rule), `record_payment`, `get_payment_history`
2. Frontend:
   - Employee salary overview (calculated vs. paid this month)
   - "Mark as paid" action → logs to `salary_payments`
   - Payment history per employee

**Deliverable:** Monthly salary auto-calculated from attendance, with a payment log.

---

## Phase 10 — Admin / Owner Dashboard

**Depends on:** Billing, Expenses, Salary all having real data (built last so it has something to show).

1. Backend commands: `get_dashboard_summary(date_range)` — aggregates sales, expenses, salary paid, profit
2. Frontend:
   - Today's snapshot: sales, expenses, net profit
   - Monthly trend chart
   - Quick links into each enabled module
   - This screen also respects module toggles — a card for a disabled module simply doesn't render

**Deliverable:** One-glance business health view for the owner.

---

## Phase 11 — Roles & Permissions (Cross-Cutting)

Can be layered in progressively (basic version early, refined later):

1. Login screen — PIN or username/password (kept simple, local-only auth, no cloud dependency)
2. Roles: Owner/Admin (sees everything, including Settings/module toggles), Cashier/Staff (sees only Billing + maybe Inventory view)
3. Route guards combine **role** + **enabled module** — a module can be enabled system-wide but still hidden from a cashier role

---

## Phase 12 — Android App

**Decision point before starting:** Evaluate Tauri Mobile (shares React/Rust core with desktop) vs. a separate lightweight app (e.g. React Native or Kotlin). Recommendation: **try Tauri Mobile first** — same codebase, same module-config system, least duplicated work; fall back to a separate app only if Tauri Mobile has blocking limitations for your target Android versions/hardware (e.g. printer access).

1. Reuse existing Rust commands + DB schema where possible
2. Local SQLite on-device (same schema, standalone per install — no forced sync to desktop unless explicitly required later)
3. Apply the **same `enabled_modules` config system**, but with an `android_enabled` flag per module so a client can show fewer modules on mobile than desktop
4. Prioritize mobile UI for: Billing (if staff will bill from phone), Dashboard (if owner just wants to check numbers on the go), Reports
5. Lower priority for mobile: Table Management, full Inventory editing (likely desktop-only tasks)

**Deliverable:** Android app with the same core modules, independently configurable visibility, targeting the use cases actually needed on mobile (this should be scoped with the client, not built as "everything desktop has").

---

## Phase 13 — Polish, Testing & Hardening

1. **Data integrity:** Wrap multi-step DB writes (e.g. sale + stock decrement) in SQLite transactions — a crash mid-sale should not leave partial data
2. **Offline resilience testing:** Kill network mid-use, kill app mid-sale, restart — verify no data loss
3. **Low-spec hardware testing:** Run on an actual old/low-RAM machine, not just dev machine — measure startup time and memory use
4. **Printer testing:** Test with real ESC/POS thermal printer, not just PDF fallback
5. **Basic automated tests:** At minimum, unit tests on Rust commands handling money/stock math (rounding, discounts, stock decrement edge cases)
6. **Error handling & user feedback:** Every failed DB write shows a clear message to the cashier — never a silent failure at checkout

---

## Phase 14 — Packaging & Deployment (Per Client)

1. Build production Tauri bundle (`tauri build`) — produces installer for target OS (Windows likely primary for local shops, macOS if relevant)
2. Client onboarding checklist:
   - Install app
   - Run first-time setup wizard: business name, business type, currency, tax rate
   - Toggle enabled modules for this client (desktop + Android separately)
   - Seed initial inventory (manual entry or CSV import — consider adding CSV import to Inventory module if clients have existing stock lists)
   - Configure/pair thermal printer
3. Android: build signed APK per client (or a single APK with in-app module config, decided in Phase 12)
4. Document a basic "admin manual" per client — how to add items, complete a sale, view reports (non-technical language)

---

## Suggested Overall Order (Summary)

```
Phase 0  → Setup & architecture decisions
Phase 1  → Module config system (build this before any feature module)
Phase 2  → Full DB schema design
Phase 3  → Inventory
Phase 4  → Billing / POS
Phase 5  → Reports
Phase 6  → Table Management (if restaurant)
Phase 7  → Attendance
Phase 8  → Expense Tracker
Phase 9  → Salary Management
Phase 10 → Admin Dashboard
Phase 11 → Roles & Permissions (layered in progressively from Phase 3 onward in practice)
Phase 12 → Android App
Phase 13 → Testing & Hardening
Phase 14 → Packaging & Client Deployment
```

## Key Principle Throughout

Every module built after Phase 1 should be built **assuming it might be toggled off** for some client — i.e., no other module should ever hard-depend on a non-core module existing (e.g. Dashboard should gracefully handle Table Management being disabled and simply not show that data, not break).