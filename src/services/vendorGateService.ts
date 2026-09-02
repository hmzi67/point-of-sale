import { call } from "./tauriClient";

/** Whether the first-run vendor gate still applies on this install
 * (`required`), and whether this app process has already cleared it
 * (`authorized`). Both come from Rust — see `vendor_gate.rs` — and the
 * frontend only ever uses them to decide what to render. The gate's real
 * enforcement is in `update_app_config`, which refuses to complete setup
 * without a grant, so nothing here is load-bearing for security. */
export interface VendorGateStatus {
  required: boolean;
  authorized: boolean;
}

export function getVendorGateStatus(): Promise<VendorGateStatus> {
  return call<VendorGateStatus>("vendor_gate_status");
}

/** Submits the vendor authorization password for verification in Rust.
 * Resolves on success (the grant is recorded backend-side for the rest of
 * the process); rejects with a generic "Incorrect authorization password"
 * otherwise. A wrong password is deliberately slow to come back — the
 * backend applies an escalating delay before rejecting. */
export function verifyVendorGate(password: string): Promise<void> {
  return call<void>("vendor_gate_verify", { password });
}
