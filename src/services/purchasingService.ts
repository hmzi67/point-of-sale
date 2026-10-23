import type {
  GoodsReceipt,
  GrnInput,
  IncomingLine,
  LedgerSummary,
  PoInput,
  PoStatus,
  PurchaseOrder,
  Statement,
  Supplier,
  SupplierInput,
  SupplierPayment,
  SupplierPaymentInput,
} from "../types";
import { call } from "./tauriClient";

export function listSuppliers(includeInactive = false): Promise<Supplier[]> {
  return call<Supplier[]>("purchasing_list_suppliers", { includeInactive });
}

export function addSupplier(input: SupplierInput): Promise<Supplier> {
  return call<Supplier>("purchasing_add_supplier", { input });
}

export function updateSupplier(id: number, input: SupplierInput): Promise<Supplier> {
  return call<Supplier>("purchasing_update_supplier", { id, input });
}

export function setSupplierActive(id: number, active: boolean): Promise<Supplier> {
  return call<Supplier>("purchasing_set_supplier_active", { id, active });
}

export function listPurchaseOrders(status: PoStatus | null): Promise<PurchaseOrder[]> {
  return call<PurchaseOrder[]>("purchasing_list_orders", { status });
}

export function createPurchaseOrder(input: PoInput): Promise<PurchaseOrder> {
  return call<PurchaseOrder>("purchasing_create_order", { input });
}

/** Draft orders only — placed orders are fixed. */
export function updatePurchaseOrder(id: number, input: PoInput): Promise<PurchaseOrder> {
  return call<PurchaseOrder>("purchasing_update_order", { id, input });
}

export function setPurchaseOrderStatus(id: number, status: "ordered" | "cancelled"): Promise<PurchaseOrder> {
  return call<PurchaseOrder>("purchasing_set_order_status", { id, status });
}

/** Ordered lines still waiting on delivery. */
export function getIncomingStock(): Promise<IncomingLine[]> {
  return call<IncomingLine[]>("purchasing_get_incoming", {});
}

/** Records a goods received note: stock goes up, item cost updates. */
export function receiveGoods(input: GrnInput): Promise<GoodsReceipt> {
  return call<GoodsReceipt>("purchasing_receive_goods", { input });
}

export function listGoodsReceipts(supplierId: number | null): Promise<GoodsReceipt[]> {
  return call<GoodsReceipt[]>("purchasing_list_receipts", { supplierId });
}

export function recordSupplierPayment(input: SupplierPaymentInput): Promise<SupplierPayment> {
  return call<SupplierPayment>("purchasing_record_payment", { input });
}

export function listSupplierPayments(supplierId: number | null): Promise<SupplierPayment[]> {
  return call<SupplierPayment[]>("purchasing_list_payments", { supplierId });
}

export function getSupplierStatement(
  supplierId: number,
  startDate: string | null,
  endDate: string | null,
): Promise<Statement> {
  return call<Statement>("purchasing_get_statement", { supplierId, startDate, endDate });
}

export function getLedgerSummary(): Promise<LedgerSummary> {
  return call<LedgerSummary>("ledger_get_summary", {});
}
