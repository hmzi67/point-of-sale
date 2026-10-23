/** Suppliers, purchase orders, goods received notes — see `db/purchasing.rs`. */

export interface Supplier {
  id: number;
  name: string;
  contactPerson: string | null;
  phone: string | null;
  email: string | null;
  address: string | null;
  notes: string | null;
  isActive: boolean;
  createdAt: string;
  purchasedMinor: number;
  paidMinor: number;
  /** What the shop still owes this supplier. */
  balanceMinor: number;
}

export interface SupplierInput {
  name: string;
  contactPerson: string | null;
  phone: string | null;
  email: string | null;
  address: string | null;
  notes: string | null;
}

export type PoStatus = "draft" | "ordered" | "partial" | "received" | "cancelled";

export interface PoLine {
  id: number;
  itemId: number;
  itemName: string;
  unit: string | null;
  qtyOrdered: number;
  qtyReceived: number;
  unitCostMinor: number;
  lineTotalMinor: number;
}

export interface PurchaseOrder {
  id: number;
  supplierId: number;
  supplierName: string;
  status: PoStatus;
  orderDate: string;
  expectedDate: string | null;
  notes: string | null;
  createdByName: string | null;
  createdAt: string;
  totalMinor: number;
  lines: PoLine[];
}

export interface PoInput {
  supplierId: number;
  orderDate: string;
  expectedDate: string | null;
  notes: string | null;
  lines: { itemId: number; qty: number; unitCostMinor: number }[];
  /** Create straight into "ordered" rather than "draft". */
  place: boolean;
}

export interface GrnLineInput {
  itemId: number;
  poItemId: number | null;
  qty: number;
  unitCostMinor: number;
  batchNo: string | null;
  expiryDate: string | null;
}

export interface GrnInput {
  supplierId: number;
  poId: number | null;
  receivedDate: string;
  invoiceRef: string | null;
  notes: string | null;
  lines: GrnLineInput[];
}

export interface GrnLine {
  itemId: number;
  itemName: string;
  unit: string | null;
  qty: number;
  unitCostMinor: number;
  lineTotalMinor: number;
  batchNo: string | null;
  expiryDate: string | null;
}

export interface GoodsReceipt {
  id: number;
  supplierId: number;
  supplierName: string;
  poId: number | null;
  receivedDate: string;
  invoiceRef: string | null;
  notes: string | null;
  totalCostMinor: number;
  receivedByName: string | null;
  createdAt: string;
  lines: GrnLine[];
}

export type SupplierPaymentMethod = "cash" | "card" | "bank" | "other";

export interface SupplierPaymentInput {
  supplierId: number;
  amountMinor: number;
  method: SupplierPaymentMethod;
  reference: string | null;
  note: string | null;
  paidDate: string;
}

export interface SupplierPayment {
  id: number;
  supplierId: number;
  supplierName: string;
  amountMinor: number;
  method: SupplierPaymentMethod;
  reference: string | null;
  note: string | null;
  paidDate: string;
  createdByName: string | null;
}

export interface IncomingLine {
  poId: number;
  poItemId: number;
  supplierName: string;
  itemId: number;
  itemName: string;
  unit: string | null;
  qtyOutstanding: number;
  unitCostMinor: number;
  expectedDate: string | null;
}

export interface LedgerParty {
  id: number;
  name: string;
  phone: string | null;
  balanceMinor: number;
  overdueMinor: number;
}

export interface LedgerSummary {
  receivableMinor: number;
  overdueMinor: number;
  overdue: import("./customers").OverdueSummary;
  payableMinor: number;
  collectedThisMonthMinor: number;
  paidOutThisMonthMinor: number;
  debtors: LedgerParty[];
  creditors: LedgerParty[];
}
