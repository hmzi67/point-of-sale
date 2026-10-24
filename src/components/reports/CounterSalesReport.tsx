import { useReport } from "../../hooks/useReport";
import { getCounterSalesSummary } from "../../services/reportsService";
import { formatMinor, minorToDecimal } from "../../utils/format";
import { DataTable, EmptyRow } from "../ui/DataTable";
import { CsvButton, ReportState, StatCard, StatGrid } from "./ReportBits";

interface Props {
  startDate: string;
  endDate: string;
  currency: string;
}

/**
 * "Counter Wise Sales": the range's sales per counter (POS terminal) and per
 * cashier. On a counter connected to its business every counter's sales are
 * in this database, so this is the whole restaurant; the rows add up to the
 * Overview's gross total.
 */
export function CounterSalesReport({ startDate, endDate, currency }: Props) {
  const { data, isLoading, error } = useReport(() => getCounterSalesSummary(startDate, endDate), [startDate, endDate]);
  const money = (minor: number) => formatMinor(minor, currency);

  return (
    <ReportState isLoading={isLoading && !data} error={error}>
      {data && (
        <div className="space-y-4">
          <StatGrid>
            <StatCard label="All counters" value={money(data.grandTotalMinor)} />
            <StatCard label="Counters with sales" value={String(data.counters.filter((c) => c.transactionCount > 0).length)} />
            <StatCard label="Bills" value={String(data.counters.reduce((sum, c) => sum + c.transactionCount, 0))} />
            <StatCard label="Refunded" value={money(data.counters.reduce((sum, c) => sum + c.refundsMinor, 0))} tone="danger" />
          </StatGrid>
          <div className="flex justify-end">
            <CsvButton
              filename={`counter-sales-${startDate}-to-${endDate}.csv`}
              rows={() => [
                ["Counter", "Cashier", "Bills", "Sales", "Refunds"],
                ...data.counters.map((c) => [c.label, "", c.transactionCount, minorToDecimal(c.totalMinor), minorToDecimal(c.refundsMinor)]),
                ...data.cashiers.map((c) => [c.counter, c.cashier, c.transactionCount, minorToDecimal(c.totalMinor), ""]),
              ]}
            />
          </div>
          <DataTable head={["Counter", "Bills", "Sales", "Refunds", "Share"]}>
            {data.counters.map((row) => {
              const share = data.grandTotalMinor > 0 ? (row.totalMinor / data.grandTotalMinor) * 100 : 0;
              return (
                <tr key={row.label} className="tabular-nums">
                  <td className="px-4 py-3 font-medium text-fg">
                    {row.label}
                    {row.thisCounter && <span className="ml-2 rounded bg-accent-soft px-1.5 py-0.5 text-xs text-accent-text">this counter</span>}
                  </td>
                  <td className="px-4 py-3 text-right">{row.transactionCount}</td>
                  <td className="px-4 py-3 text-right">{money(row.totalMinor)}</td>
                  <td className="px-4 py-3 text-right">{row.refundsMinor ? money(row.refundsMinor) : "—"}</td>
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
            {data.counters.length === 0 && <EmptyRow span={5} text="No sales in this period." />}
          </DataTable>
          <DataTable head={["Cashier", "Counter", "Bills", "Sales"]}>
            {data.cashiers.map((row) => (
              <tr key={`${row.counter}-${row.cashier}`} className="tabular-nums">
                <td className="px-4 py-3 font-medium text-fg">{row.cashier}</td>
                <td className="px-4 py-3 text-fg-2">{row.counter}</td>
                <td className="px-4 py-3 text-right">{row.transactionCount}</td>
                <td className="px-4 py-3 text-right">{money(row.totalMinor)}</td>
              </tr>
            ))}
            {data.cashiers.length === 0 && <EmptyRow span={4} text="No sales in this period." />}
          </DataTable>
        </div>
      )}
    </ReportState>
  );
}
