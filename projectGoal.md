# Diwan — Project Goals

## Overview
A **generic, white-label-ready** Point of Sale (POS) system — not a single-shop tool, but a product that can be configured and sold to multiple clients, each with different needs (retail shop, restaurant, etc.). Built desktop-first, designed to run entirely offline and perform smoothly on low-spec hardware commonly found in small local businesses. An Android companion app is also planned so clients who want mobile access (e.g. owner checking sales on phone, or a lightweight mobile billing counter) can get one from the same core system.

## Product Philosophy — Build Once, Configure Per Client

This is the most important shift in direction: **this is not a custom app for one shop — it's a reusable product.**

- Every client may need a different combination of modules. One client (retail shop) may not need Table Management or Employee Attendance at all; another (restaurant) may need everything.
- Instead of writing separate versions per client, the system should have **all modules built-in but toggleable** — enabled/disabled per client/installation without touching code.
- The Android app should be able to show/hide the same modules independently — e.g. a client may want full features on desktop but only Billing + Reports visible on their staff's Android app.
- This means architecture decisions (DB schema, UI navigation, permissions) must be made with **configurability in mind from day one**, not bolted on later.

## Core Objectives

1. **Offline-First Operation**
   All core functionality (billing, inventory, reports, attendance, expenses) must work with zero internet dependency. The app should never block a sale or freeze because of a lost connection.

2. **Performance on Low-Spec Hardware**
   Fast startup, low RAM/CPU usage, and no reliance on bundled browser runtimes. The app should feel instant on old shop PCs, not just modern dev machines.

3. **Reliability for Daily Business Use**
   A shop cannot afford data loss or crashes mid-sale. Local SQLite storage with proper transaction handling ensures every bill, stock update, and record survives power cuts, app crashes, or accidental closes.

4. **Simple, Fast Workflow for Non-Technical Staff**
   Cashiers and shop staff are not tech-savvy. The billing screen especially must be fast (keyboard/barcode-friendly) and require minimal clicks to complete a sale.

5. **Single Source of Truth for Shop Operations**
   Inventory, billing, attendance, expenses, and salary all pull from and write to the same local database — no duplicate data entry, no disconnected spreadsheets.

6. **Generic & Reusable Across Clients**
   No hardcoded business logic tied to one shop's workflow. Business type (retail/restaurant), enabled modules, and branding should be configuration, not code changes.

7. **Per-Client, Per-Platform Module Control**
   Each installation (desktop or Android) should have its own independent set of enabled/hidden modules — e.g. Client A sees everything on desktop; Client B's Android app shows only Billing and Reports.

8. **Cross-Platform from Shared Core**
   Desktop (Tauri) and Android app should ideally share the same backend logic/data model concepts, so features built once are reusable across platforms rather than rebuilt separately.

## Core Modules & Their Goals

All modules below are **optional/toggleable per client and per platform** — none are hardcoded as "always on" except Billing/POS, which is the minimum viable core.

| Module | Goal | Toggleable? |
|---|---|---|
| **Inventory Management** | Track stock accurately in real time; alert on low stock; organize items by category | Yes |
| **Billing / POS** | Fast checkout — search/scan item, add to cart, apply discount, print receipt in seconds | Core (always on) |
| **Daily Reports** | Give the owner a clear daily/monthly view of sales and top-selling items, exportable as PDF/Excel | Yes |
| **Table Management** | (Restaurant use case) Track table status and map orders to tables without confusion | Yes |
| **Employee Attendance** | Simple check-in/check-out logging with monthly summaries | Yes |
| **Expense Tracker** | Log daily shop expenses by category for accurate profit calculation | Yes |
| **Salary Management** | Auto-calculate salaries based on attendance; maintain a payment history log | Yes |
| **Admin/Owner Dashboard** | One combined view of sales, expenses, and profit — the "how is my shop doing today/this month" screen | Yes |

## Feature Configuration System (Key Architectural Requirement)

To support multiple clients from one codebase, the system needs a **module/feature toggle mechanism**:

- A config table (e.g. `enabled_modules`) in the local DB, or a settings/config file, storing which modules are active for that specific installation.
- Navigation/sidebar UI should be built dynamically from this config — a hidden module should not appear in menus, and its routes should not be accessible.
- Same toggle concept should apply to the Android app, likely synced from the same client config (via local settings or an initial setup/license step) so each client's mobile experience matches what they've paid for / need.
- This should be designed early — retrofitting toggles into a rigid navigation/DB structure later is expensive. Schema and routing should assume "modules are dynamic" from the very first implementation.

## Success Criteria

- App launches in under ~2 seconds on a low-spec PC
- Billing a sale takes under 10 seconds for a trained cashier
- Zero data loss on unexpected shutdown (SQLite transactions + local persistence)
- App works fully with no internet connection at any point
- Owner can see daily profit/loss at a glance from the dashboard
- App size stays small (Tauri advantage — no bundled Chromium)
- A new client can be onboarded by **configuring modules**, not writing new code
- Hiding/showing a module (desktop or Android) requires no rebuild — just a config change

## Non-Goals (for now)

- Cloud sync / multi-branch sync (optional future addition, not core)
- Multi-language support (can be added later if needed)
- Online ordering / e-commerce integration
- Full multi-tenant cloud backend (each client still runs local/offline — "generic" here means *reusable codebase*, not *centrally hosted SaaS*)

## Tech Stack Reference

**Desktop (primary platform)**
- Frontend: Tauri + React + TypeScript + TailwindCSS
- Backend logic: Rust (via Tauri commands)
- Database: SQLite (local, offline)
- Printing: ESC/POS thermal printer integration

**Android (planned companion app)**
- To be evaluated: Tauri Mobile (shares React frontend + Rust logic with desktop) vs. a separate lightweight app.
- Should reuse the same data model/schema concepts as desktop wherever possible to avoid duplicated business logic.
- Local storage on Android (SQLite) with the same config-driven module visibility as desktop.   