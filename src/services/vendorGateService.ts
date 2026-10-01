import type { VendorGateStatus } from "../types";
import { call } from "./tauriClient";

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
