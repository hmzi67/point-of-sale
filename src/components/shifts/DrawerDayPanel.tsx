import { useEffect, useState } from "react";
import { getDrawerDay } from "../../services/shiftsService";
import { useWorkspaceDataChanged } from "../../hooks/useWorkspaceDataChanged";
import { todayString } from "../../utils/dateRanges";
import { formatMinor } from "../../utils/format";
import type { DrawerDay } from "../../types";
import { DataTable, EmptyRow } from "../ui/DataTable";

/**
 * Closing the day across every cash drawer: each counter's shifts opened on
 * the chosen day, then the totals. On a counter connected to its business
 * the other counters' shifts and sales are here too, so this covers the
 * whole restaurant; on a single counter it is simply that day's shifts.
 */
export function DrawerDayPanel({ currency }: { currency: string }) {
  const [day, setDay] = useState(todayString());
  const [data, setData] = useState<DrawerDay | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reloads, setReloads] = useState(0);
  useWorkspaceDataChanged(() => setReloads((n) => n + 1));

  useEffect(() => {
    let cancelled = false;
    getDrawerDay(day)
      .then((result) => {
        if (!cancelled) {
          setData(result);
          setError(null);
        }
      })
      .catch((e: Error) => {
        if (!cancelled) setError(e.message);
      });
    return () => {
      cancelled = true;
    };
  }, [day, reloads]);

  const money = (minor: number) => formatMinor(minor, currency);
  const differenceTone = (minor: number) => (minor < 0 ? "text-danger-text" : minor > 0 ? "text-success-text" : "text-fg-2");

  return (
    <div className="space-y-3 rounded-lg border border-line bg-surface p-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold text-fg">Day across all drawers</h3>
          <p className="text-xs text-fg-muted">Every counter's cash drawer for the day, for closing up.</p>
        </div>
        <label className="text-xs text-fg-muted">
          Day
          <input
            type="date"
            value={day}
            max={todayString()}
            onChange={(e) => e.target.value && setDay(e.target.value)}
            className="ml-2 rounded-md border border-line bg-surface-2 px-2 py-1 text-sm text-fg"
          />
        </label>
      </div>

      {error && <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      {data && (
        <>
          <DataTable head={["Drawer", "Cashier", "Opening", "Cash sales", "All sales", "Expected cash", "Counted", "Short / over"]}>
            {data.drawers.length === 0 && <EmptyRow span={8} text="No shift was opened on this day." />}
            {data.drawers.map((d) => (
              <tr key={d.shift.id}>
                <td className="whitespace-nowrap px-4 py-2.5 text-fg">
                  {d.shift.tillName ?? "This counter"}
                  <span className="block text-xs text-fg-muted">
                    #{d.shift.id} · {d.shift.closedAt ? `closed ${d.shift.closedAt.slice(11, 16)}` : "still open"}
                  </span>
                </td>
                <td className="px-4 py-2.5 text-fg-2">{d.shift.cashierName ?? "—"}</td>
                <td className="px-4 py-2.5 text-fg-2">{money(d.openingBalanceMinor)}</td>
                <td className="px-4 py-2.5 text-fg-2">{money(d.cashSalesMinor)}</td>
                <td className="px-4 py-2.5 text-fg-2">{money(d.totalSalesMinor)}</td>
                <td className="px-4 py-2.5 font-medium text-fg">{money(d.expectedCashMinor)}</td>
                <td className="px-4 py-2.5 text-fg-2">{d.declaredCashAmountMinor === null ? "—" : money(d.declaredCashAmountMinor)}</td>
                <td className={`px-4 py-2.5 font-medium ${d.differenceMinor === null ? "text-fg-muted" : differenceTone(d.differenceMinor)}`}>
                  {d.differenceMinor === null ? "—" : `${d.differenceMinor < 0 ? "Short " : d.differenceMinor > 0 ? "Over " : ""}${money(Math.abs(d.differenceMinor))}`}
                </td>
              </tr>
            ))}
          </DataTable>

          {data.drawers.length > 0 && (
            <dl className="grid grid-cols-2 gap-3 text-sm sm:grid-cols-4">
              <Total label="All sales" value={money(data.totalSalesMinor)} />
              <Total label="Cash expected in drawers" value={money(data.expectedCashMinor)} />
              <Total label="Card / credit / other" value={money(data.cardSalesMinor + data.creditSalesMinor + data.otherSalesMinor)} />
              <Total
                label={data.openDrawers > 0 ? `Short / over (${data.openDrawers} still open)` : "Short / over"}
                value={`${data.differenceMinor < 0 ? "Short " : data.differenceMinor > 0 ? "Over " : ""}${money(Math.abs(data.differenceMinor))}`}
                tone={differenceTone(data.differenceMinor)}
              />
            </dl>
          )}
        </>
      )}
    </div>
  );
}

function Total({ label, value, tone }: { label: string; value: string; tone?: string }) {
  return (
    <div className="rounded-lg border border-line/70 bg-surface-2 p-3">
      <dt className="text-xs text-fg-muted">{label}</dt>
      <dd className={`mt-1 font-semibold ${tone ?? "text-fg"}`}>{value}</dd>
    </div>
  );
}
