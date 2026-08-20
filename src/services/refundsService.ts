import type { Refund, RefundableSale, RefundLineInput } from "../types";
import { call } from "./tauriClient";

/** The original sale plus, per line, how much of it is still refundable. */
export function getSaleForRefund(saleId: number): Promise<RefundableSale> {
  return call<RefundableSale>("refund_get_sale", { saleId });
}

/** Creates a refund — re-validated server-side against the live sale/prior
 * refunds; also puts the refunded quantity back onto stock. `refundedBy` is
 * never sent — the server always uses the signed-in caller. */
export function createRefund(saleId: number, items: RefundLineInput[], reason: string | null): Promise<Refund> {
  return call<Refund>("refund_create", { saleId, items, reason });
}

/** Re-fetches a previously created refund — used to reprint its receipt. */
export function getRefund(refundId: number): Promise<Refund> {
  return call<Refund>("refund_get", { refundId });
}

/** Prints the "Refund Details" receipt on a USB thermal printer. */
export function printRefundThermal(refundId: number): Promise<void> {
  return call<void>("refund_print_thermal", { refundId });
}
