# services

One module per backend area (`inventoryService.ts`, `billingService.ts`, …).
Every function here calls `call()` from `tauriClient.ts`; components import
these services and never `@tauri-apps/api` directly.
