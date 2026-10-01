/** Whether the first-run vendor gate still applies on this install
 * (`required`), and whether this app process has already cleared it
 * (`authorized`). Both come from Rust — see `vendor_gate.rs` — and the
 * frontend only ever uses them to decide what to render. The gate's real
 * enforcement is in `config_update`, which refuses to complete setup
 * without a grant, so nothing here is load-bearing for security. */
export interface VendorGateStatus {
  required: boolean;
  authorized: boolean;
}
