import { useState } from "react";
import { useReport } from "../../hooks/useReport";
import { getOrderProfit, getPaymentTypes, getProfitLoss, getTaxReport, getVoidReport } from "../../services/insightsService";
import type { PeriodGrouping } from "../../types";
import { paymentMethodLabel } from "../../utils/checkoutPayments";
import { formatMinor, formatQty, minorToDecimal } from "../../utils/format";
import { DataTable, EmptyRow } from "../ui/DataTable";
import { CsvButton, ReportState, SegmentedControl, StatCard, StatGrid } from "./ReportBits";

interface RangeProps {
  startDate: string;
  endDate: string;
  currency: string;
}

const dec = minorToDecimal;

/** Profit & loss for the range, with a daily/weekly/monthly breakdown. */
export function ProfitLossReport({ startDate, endDate, currency }: RangeProps) {
  const [grouping, setGrouping] = useState<PeriodGrouping>("day");
  const { data, isLoading, error } = useReport(() => getProfitLoss(startDate, endDate, grouping), [startDate, endDate, grouping]);
  const money = (minor: number) => formatMinor(minor, currency);

  return (
    <ReportState isLoading={isLoading && !data} error={error}>
      {data && (
        <div className="space-y-5">
          <StatGrid>
            <StatCard label="Net revenue" value={money(data.netRevenueMinor)} detail={`${data.transactions} sales`} />
            <StatCard label="Cost of goods" value={money(data.cogsMinor)} />
            <StatCard
              label="Gross profit"
              value={money(data.grossProfitMinor)}
              detail={data.netRevenueMinor > 0 ? `${((data.grossProfitMinor / data.netRevenueMinor) * 100).toFixed(1)}% margin` : undefined}
              tone={data.grossProfitMinor >= 0 ? "success" : "danger"}
            />
            <StatCard label="Net profit" value={money(data.netProfitMinor)} tone={data.netProfitMinor >= 0 ? "success" : "danger"} />
          </StatGrid>

          <div className="grid grid-cols-1 gap-5 lg:grid-cols-[22rem_1fr]">
            <div className="rounded-2xl border border-line bg-surface p-5 text-sm">
              <h3 className="mb-3 text-sm font-semibold text-fg">Profit & loss statement</h3>
              <dl className="space-y-1.5 tabular-nums">
                <Line label="Gross sales" value={money(data.grossSalesMinor)} />
                <Line label="Less discounts" value={`-${money(data.discountsMinor)}`} muted />
                <Line label="Less returns & refunds" value={`-${money(data.refundsMinor)}`} muted />
                <Line label="Service charges" value={money(data.serviceChargeMinor)} muted />
                <Line label="Net revenue" value={money(data.netRevenueMinor)} strong />
                <Line label="Cost of goods sold" value={`-${money(data.cogsMinor)}`} muted />
                <Line label="Gross profit" value={money(data.grossProfitMinor)} strong />
                {data.expensesMinor !== null && <Line label="Expenses" value={`-${money(data.expensesMinor)}`} muted />}
                {data.salariesMinor !== null && <Line label="Salaries" value={`-${money(data.salariesMinor)}`} muted />}
                <Line label="Net profit" value={money(data.netProfitMinor)} strong />
              </dl>
              <p className="mt-3 text-[11px] text-fg-muted">Tax collected ({money(data.taxMinor)}) is owed to the government and isn't counted as revenue.</p>
            </div>

            <div className="space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <SegmentedControl
                  value={grouping}
                  onChange={setGrouping}
                  options={[
                    { value: "day", label: "Daily" },
                    { value: "week", label: "Weekly" },
                    { value: "month", label: "Monthly" },
                  ]}
                />
                <CsvButton
                  filename={`profit-loss-${startDate}-to-${endDate}.csv`}
                  rows={() => [
                    ["Period", "Sales", "Net revenue", "COGS", "Gross profit", "Expenses + salaries", "Net profit"],
                    ...data.periods.map((p) => [p.period, p.transactions, dec(p.netRevenueMinor), dec(p.cogsMinor), dec(p.grossProfitMinor), dec(p.expensesMinor), dec(p.netProfitMinor)]),
                  ]}
                />
              </div>
              <DataTable head={["Period", "Sales", "Revenue", "COGS", "Gross profit", "Expenses", "Net profit"]}>
                {data.periods.map((p) => (
                  <tr key={p.period} className="tabular-nums">
                    <td className="px-4 py-2.5 font-medium text-fg">{p.period}</td>
                    <td className="px-4 py-2.5 text-right">{p.transactions}</td>
                    <td className="px-4 py-2.5 text-right">{money(p.netRevenueMinor)}</td>
                    <td className="px-4 py-2.5 text-right text-fg-muted">{money(p.cogsMinor)}</td>
                    <td className="px-4 py-2.5 text-right">{money(p.grossProfitMinor)}</td>
                    <td className="px-4 py-2.5 text-right text-fg-muted">{money(p.expensesMinor)}</td>
                    <td className={`px-4 py-2.5 text-right font-semibold ${p.netProfitMinor < 0 ? "text-danger-text" : "text-success-text"}`}>{money(p.netProfitMinor)}</td>
                  </tr>
                ))}
                {data.periods.length === 0 && <EmptyRow span={7} text="No activity in this period." />}
              </DataTable>
            </div>
          </div>
        </div>
      )}
    </ReportState>
  );
}

function Line({ label, value, strong, muted }: { label: string; value: string; strong?: boolean; muted?: boolean }) {
  return (
    <div className={`flex justify-between ${strong ? "border-t border-line pt-1.5 font-semibold text-fg" : muted ? "text-fg-muted" : "text-fg-2"}`}>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

/** Profit per sale. */
export function OrderProfitReport({ startDate, endDate, currency }: RangeProps) {
  const { data, isLoading, error } = useReport(() => getOrderProfit(startDate, endDate), [startDate, endDate]);
  const money = (minor: number) => formatMinor(minor, currency);
  const totals = (data ?? []).reduce(
    (acc, row) => ({ revenue: acc.revenue + row.revenueMinor - row.refundedMinor, cost: acc.cost + row.costMinor, profit: acc.profit + row.profitMinor }),
    { revenue: 0, cost: 0, profit: 0 },
  );

  return (
    <ReportState isLoading={isLoading && !data} error={error}>
      {data && (
        <div className="space-y-4">
          <StatGrid>
            <StatCard label="Sales" value={String(data.length)} detail={data.length === 2000 ? "Showing the latest 2000" : undefined} />
            <StatCard label="Revenue (net of refunds)" value={money(totals.revenue)} />
            <StatCard label="Cost" value={money(totals.cost)} />
            <StatCard label="Profit" value={money(totals.profit)} tone={totals.profit >= 0 ? "success" : "danger"} />
          </StatGrid>
          <div className="flex justify-end">
            <CsvButton
              filename={`order-profit-${startDate}-to-${endDate}.csv`}
              rows={() => [
                ["Sale", "Order", "Date", "Cashier", "Customer", "Payment", "Revenue", "Refunded", "Cost", "Profit", "Margin %"],
                ...data.map((r) => [r.saleId, r.orderNo ?? "", r.createdAt, r.cashierName ?? "", r.customerName ?? "", r.paymentMethod, dec(r.revenueMinor), dec(r.refundedMinor), dec(r.costMinor), dec(r.profitMinor), r.marginPercent]),
              ]}
            />
          </div>
          <DataTable head={["Sale", "When", "Cashier / customer", "Paid by", "Revenue", "Cost", "Profit", "Margin"]}>
            {data.map((row) => (
              <tr key={row.saleId} className="tabular-nums">
                <td className="px-4 py-2.5 font-medium text-fg">#{row.saleId}{row.orderNo && <span className="block text-xs text-fg-muted">Order {row.orderNo}</span>}</td>
                <td className="whitespace-nowrap px-4 py-2.5 text-xs text-fg-muted">{row.createdAt}</td>
                <td className="px-4 py-2.5 text-xs text-fg-2">{row.cashierName ?? "—"}{row.customerName && <span className="block text-fg-muted">{row.customerName}</span>}</td>
                <td className="px-4 py-2.5 text-xs">{paymentMethodLabel(row.paymentMethod)}</td>
                <td className="px-4 py-2.5 text-right">
                  {money(row.revenueMinor)}
                  {row.refundedMinor > 0 && <span className="block text-xs text-danger-text">-{money(row.refundedMinor)}</span>}
                </td>
                <td className="px-4 py-2.5 text-right text-fg-muted">{money(row.costMinor)}</td>
                <td className={`px-4 py-2.5 text-right font-semibold ${row.profitMinor < 0 ? "text-danger-text" : "text-success-text"}`}>{money(row.profitMinor)}</td>
                <td className="px-4 py-2.5 text-right text-fg-2">{row.marginPercent}%</td>
              </tr>
            ))}
            {data.length === 0 && <EmptyRow span={8} text="No sales in this period." />}
          </DataTable>
        </div>
      )}
    </ReportState>
  );
}

/** Takings by payment type, including the parts of split bills and
 * pay-later installments collected. */
export function PaymentTypeReport({ startDate, endDate, currency }: RangeProps) {
  const { data, isLoading, error } = useReport(() => getPaymentTypes(startDate, endDate), [startDate, endDate]);
  const money = (minor: number) => formatMinor(minor, currency);
  const total = (data?.rows ?? []).reduce((sum, r) => sum + r.amountMinor, 0);

  return (
    <ReportState isLoading={isLoading && !data} error={error}>
      {data && (
        <div className="space-y-4">
          <StatGrid>
            <StatCard label="Taken at the till" value={money(total)} />
            <StatCard label="Split-payment bills" value={String(data.splitSaleCount)} />
            <StatCard label="Credit collected" value={money(data.collections.reduce((s, r) => s + r.amountMinor, 0))} detail="Installments on pay-later bills" tone="success" />
            <StatCard label="Refunded" value={money(data.refundsMinor)} tone="danger" />
          </StatGrid>
          <div className="flex justify-end">
            <CsvButton
              filename={`payment-types-${startDate}-to-${endDate}.csv`}
              rows={() => [
                ["Kind", "Method", "Amount", "Bills"],
                ...data.rows.map((r) => ["Sales", paymentMethodLabel(r.method), dec(r.amountMinor), r.saleCount]),
                ...data.collections.map((r) => ["Credit collected", paymentMethodLabel(r.method), dec(r.amountMinor), r.saleCount]),
              ]}
            />
          </div>
          <DataTable head={["Payment type", "Bills", "Amount", "Share"]}>
            {data.rows.map((row) => {
              const share = total > 0 ? (row.amountMinor / total) * 100 : 0;
              return (
                <tr key={row.method} className="tabular-nums">
                  <td className="px-4 py-3 font-medium text-fg">{paymentMethodLabel(row.method)}</td>
                  <td className="px-4 py-3 text-right">{row.saleCount}</td>
                  <td className="px-4 py-3 text-right">{money(row.amountMinor)}</td>
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-2">
                      <div className="h-2 w-32 overflow-hidden rounded-full bg-surface-3">
                        <div className="h-full rounded-full bg-accent" style={{ width: `${share}%` }} />
                      </div>
                      <span className="text-xs text-fg-muted">{share.toFixed(1)}%</span>
                    </div>
                  </td>
                </tr>
              );
            })}
            {data.rows.length === 0 && <EmptyRow span={4} text="No sales in this period." />}
          </DataTable>
          {data.collections.length > 0 && (
            <DataTable head={["Credit collected by", "Bills", "Amount"]}>
              {data.collections.map((row) => (
                <tr key={row.method} className="tabular-nums">
                  <td className="px-4 py-3 font-medium text-fg">{paymentMethodLabel(row.method)}</td>
                  <td className="px-4 py-3 text-right">{row.saleCount}</td>
                  <td className="px-4 py-3 text-right">{money(row.amountMinor)}</td>
                </tr>
              ))}
            </DataTable>
          )}
        </div>
      )}
    </ReportState>
  );
}

/** Tax and service charge collected, day by day. */
export function TaxReportView({ startDate, endDate, currency }: RangeProps) {
  const { data, isLoading, error } = useReport(() => getTaxReport(startDate, endDate), [startDate, endDate]);
  const money = (minor: number) => formatMinor(minor, currency);

  return (
    <ReportState isLoading={isLoading && !data} error={error}>
      {data && (
        <div className="space-y-4">
          <StatGrid>
            <StatCard label="Taxable sales" value={money(data.taxableMinor)} />
            <StatCard label="Tax collected" value={money(data.taxMinor)} />
            <StatCard label="Service charges" value={money(data.serviceChargeMinor)} />
            <StatCard label="Refunds" value={money(data.refundsMinor)} detail={`${data.taxExemptSaleCount} sale(s) without tax`} tone="danger" />
          </StatGrid>
          <div className="flex justify-end">
            <CsvButton
              filename={`tax-${startDate}-to-${endDate}.csv`}
              rows={() => [
                ["Date", "Sales", "Taxable", "Tax", "Service charge", "Refunds"],
                ...data.rows.map((r) => [r.date, r.transactions, dec(r.taxableMinor), dec(r.taxMinor), dec(r.serviceChargeMinor), dec(r.refundsMinor)]),
              ]}
            />
          </div>
          <DataTable head={["Date", "Sales", "Taxable", "Tax", "Service charge", "Refunds"]}>
            {data.rows.map((row) => (
              <tr key={row.date} className="tabular-nums">
                <td className="px-4 py-2.5 font-medium text-fg">{row.date}</td>
                <td className="px-4 py-2.5 text-right">{row.transactions}</td>
                <td className="px-4 py-2.5 text-right">{money(row.taxableMinor)}</td>
                <td className="px-4 py-2.5 text-right">{money(row.taxMinor)}</td>
                <td className="px-4 py-2.5 text-right">{money(row.serviceChargeMinor)}</td>
                <td className="px-4 py-2.5 text-right text-danger-text">{row.refundsMinor ? money(row.refundsMinor) : ""}</td>
              </tr>
            ))}
            {data.rows.length === 0 && <EmptyRow span={6} text="No sales in this period." />}
          </DataTable>
        </div>
      )}
    </ReportState>
  );
}

/** Voided lines, cancelled orders and refunds — everything that didn't
 * stay sold. */
export function VoidReportView({ startDate, endDate, currency }: RangeProps) {
  const { data, isLoading, error } = useReport(() => getVoidReport(startDate, endDate), [startDate, endDate]);
  const money = (minor: number) => formatMinor(minor, currency);

  return (
    <ReportState isLoading={isLoading && !data} error={error}>
      {data && (
        <div className="space-y-4">
          <StatGrid>
            <StatCard label="Voided items" value={money(data.voidValueMinor)} detail={`${data.voids.length} line(s)`} tone="warning" />
            <StatCard label="Cancelled orders" value={money(data.cancelledValueMinor)} detail={`${data.cancelledOrders.length} order(s)`} tone="warning" />
            <StatCard label="Refunds / returns" value={money(data.refundsMinor)} detail={`${data.refundCount} refund(s)`} tone="danger" />
            <StatCard label="Total lost" value={money(data.voidValueMinor + data.cancelledValueMinor + data.refundsMinor)} />
          </StatGrid>
          <div className="flex justify-end">
            <CsvButton
              filename={`voids-${startDate}-to-${endDate}.csv`}
              rows={() => [
                ["Type", "Order", "Item / type", "Qty", "Value", "Reason", "By", "When"],
                ...data.voids.map((v) => ["Void", v.orderNo, v.itemName, v.qty, dec(v.valueMinor), v.reason ?? "", v.voidedBy ?? "", v.voidedAt]),
                ...data.cancelledOrders.map((c) => ["Cancelled order", c.orderNo, c.orderType, "", dec(c.valueMinor), c.reason ?? "", "", c.cancelledAt]),
              ]}
            />
          </div>
          <DataTable head={["Order", "Item", "Qty", "Value", "Reason", "Voided by", "When"]}>
            {data.voids.map((v, i) => (
              <tr key={i} className="tabular-nums">
                <td className="px-4 py-2.5 font-medium text-fg">#{v.orderNo}</td>
                <td className="px-4 py-2.5 text-fg-2">{v.itemName}</td>
                <td className="px-4 py-2.5">{formatQty(v.qty)}</td>
                <td className="px-4 py-2.5 text-right">{money(v.valueMinor)}</td>
                <td className="px-4 py-2.5 text-xs text-fg-muted">{v.reason ?? "—"}</td>
                <td className="px-4 py-2.5 text-xs text-fg-2">{v.voidedBy ?? "—"}</td>
                <td className="whitespace-nowrap px-4 py-2.5 text-xs text-fg-muted">{v.voidedAt}</td>
              </tr>
            ))}
            {data.voids.length === 0 && <EmptyRow span={7} text="No voided items." />}
          </DataTable>
          {data.cancelledOrders.length > 0 && (
            <DataTable head={["Cancelled order", "Type", "Value", "Reason", "When"]}>
              {data.cancelledOrders.map((c) => (
                <tr key={c.orderId} className="tabular-nums">
                  <td className="px-4 py-2.5 font-medium text-fg">#{c.orderNo}</td>
                  <td className="px-4 py-2.5 text-fg-2">{c.orderType.replace(/_/g, " ")}</td>
                  <td className="px-4 py-2.5 text-right">{money(c.valueMinor)}</td>
                  <td className="px-4 py-2.5 text-xs text-fg-muted">{c.reason ?? "—"}</td>
                  <td className="whitespace-nowrap px-4 py-2.5 text-xs text-fg-muted">{c.cancelledAt}</td>
                </tr>
              ))}
            </DataTable>
          )}
        </div>
      )}
    </ReportState>
  );
}
