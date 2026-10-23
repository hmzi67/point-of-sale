import type {
  CreditSale,
  Customer,
  CustomerInput,
  CustomerPayment,
  CustomerPaymentInput,
  OverdueSummary,
  Statement,
} from "../types";
import { call } from "./tauriClient";

export function listCustomers(search: string | null, includeInactive = false): Promise<Customer[]> {
  return call<Customer[]>("customers_list", { search, includeInactive });
}

/** The billing screen's customer picker — active customers only, a few rows. */
export function searchCustomers(search: string): Promise<Customer[]> {
  return call<Customer[]>("customers_search", { search });
}

export function getCustomer(id: number): Promise<Customer> {
  return call<Customer>("customers_get", { id });
}

export function addCustomer(input: CustomerInput): Promise<Customer> {
  return call<Customer>("customers_add", { input });
}

export function updateCustomer(id: number, input: CustomerInput): Promise<Customer> {
  return call<Customer>("customers_update", { id, input });
}

/** Archive/restore — refused while the customer still owes money. */
export function setCustomerActive(id: number, active: boolean): Promise<Customer> {
  return call<Customer>("customers_set_active", { id, active });
}

export function getCreditSales(customerId: number | null, onlyOutstanding: boolean): Promise<CreditSale[]> {
  return call<CreditSale[]>("customers_get_credit_sales", { customerId, onlyOutstanding });
}

export function getCustomerPayments(customerId: number): Promise<CustomerPayment[]> {
  return call<CustomerPayment[]>("customers_get_payments", { customerId });
}

/** Records an installment; refuses more than is owed. */
export function recordCustomerPayment(input: CustomerPaymentInput): Promise<CustomerPayment[]> {
  return call<CustomerPayment[]>("customers_record_payment", { input });
}

export function getCustomerStatement(
  customerId: number,
  startDate: string | null,
  endDate: string | null,
): Promise<Statement> {
  return call<Statement>("customers_get_statement", { customerId, startDate, endDate });
}

/** Pay-later sales past their due date — the payment alerts. */
export function getOverdueSales(): Promise<CreditSale[]> {
  return call<CreditSale[]>("customers_get_overdue", {});
}

export function getOverdueSummary(): Promise<OverdueSummary> {
  return call<OverdueSummary>("customers_get_overdue_summary", {});
}
