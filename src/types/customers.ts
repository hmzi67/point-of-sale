/** Customer accounts and pay-later (credit) sales — see `db/customers.rs`. */

export interface Customer {
  id: number;
  name: string;
  phone: string | null;
  email: string | null;
  address: string | null;
  notes: string | null;
  isActive: boolean;
  createdAt: string;
  /** What they still owe across their pay-later sales. */
  balanceMinor: number;
  /** The part of `balanceMinor` that is past its due date. */
  overdueMinor: number;
}

export interface CustomerInput {
  name: string;
  phone: string | null;
  email: string | null;
  address: string | null;
  notes: string | null;
}

export interface CreditSale {
  saleId: number;
  customerId: number;
  customerName: string;
  customerPhone: string | null;
  createdAt: string;
  dueDate: string | null;
  totalMinor: number;
  creditMinor: number;
  paidMinor: number;
  refundedMinor: number;
  outstandingMinor: number;
  /** 0 when not yet due or settled. */
  daysOverdue: number;
}

export type CollectionMethod = "cash" | "card" | "other";

export interface CustomerPayment {
  id: number;
  customerId: number;
  saleId: number | null;
  amountMinor: number;
  method: CollectionMethod;
  note: string | null;
  receivedByName: string | null;
  createdAt: string;
}

export interface CustomerPaymentInput {
  customerId: number;
  /** `null` spreads the amount over outstanding sales, oldest due first. */
  saleId: number | null;
  amountMinor: number;
  method: CollectionMethod;
  note: string | null;
}

export interface OverdueSummary {
  saleCount: number;
  customerCount: number;
  amountMinor: number;
  dueSoonCount: number;
}

/** One statement line. `debitMinor` raises the balance, `creditMinor` lowers it. */
export interface LedgerEntry {
  date: string;
  kind: string;
  reference: string;
  description: string;
  debitMinor: number;
  creditMinor: number;
  balanceMinor: number;
}

export interface Statement {
  openingBalanceMinor: number;
  entries: LedgerEntry[];
  closingBalanceMinor: number;
}
