import { useEffect, useState } from "react";
import { Download } from "lucide-react";
import type { Statement } from "../../types";
import { buildCsv, downloadTextFile } from "../../utils/csv";
import { formatMinor, minorToDecimal } from "../../utils/format";
import { runAction } from "../../utils/runAction";
import { DateRangeInputs } from "../ui/DateRangeInputs";

interface StatementViewProps {
  /** Loads the statement for an optional date range. */
  load: (startDate: string | null, endDate: string | null) => Promise<Statement>;
  currency: string;
  /** Column captions — customers owe on debit, suppliers are owed on it. */
  debitLabel: string;
  creditLabel: string;
  /** Used for the CSV file name. */
  partyName: string;
  /** Bumped by the caller after a payment so the statement reloads. */
  reloadKey?: number;
}

/** A running-balance statement with an optional date range and CSV export. */
export function StatementView({ load, currency, debitLabel, creditLabel, partyName, reloadKey = 0 }: StatementViewProps) {
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState("");
  const [statement, setStatement] = useState<Statement | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isExporting, setIsExporting] = useState(false);

  useEffect(() => {
    if (startDate && endDate && startDate > endDate) return;
    load(startDate || null, endDate || null)
      .then((result) => {
        setStatement(result);
        setError(null);
      })
      .catch((e: Error) => setError(e.message));
    // `load` is a fresh closure each render; the range and reloadKey decide.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [startDate, endDate, reloadKey]);

  const exportCsv = () =>
    runAction(setIsExporting, setError, async () => {
      if (!statement) return;
      const rows: Array<Array<string | number>> = [
        ["Date", "Reference", "Description", debitLabel, creditLabel, "Balance"],
        ["", "", "Opening balance", "", "", minorToDecimal(statement.openingBalanceMinor)],
        ...statement.entries.map((e) => [
          e.date,
          e.reference,
          e.description,
          e.debitMinor ? minorToDecimal(e.debitMinor) : "",
          e.creditMinor ? minorToDecimal(e.creditMinor) : "",
          minorToDecimal(e.balanceMinor),
        ]),
      ];
      await downloadTextFile(buildCsv(rows), `statement-${partyName.replace(/\s+/g, "-").toLowerCase()}.csv`);
    });

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <DateRangeInputs
          start={startDate}
          end={endDate}
          onStartChange={setStartDate}
          onEndChange={setEndDate}
          inputClassName="rounded-xl border border-line-strong px-3 py-2 text-sm"
        />
        {(startDate || endDate) && (
          <button type="button" onClick={() => { setStartDate(""); setEndDate(""); }} className="text-xs text-fg-muted underline">
            All time
          </button>
        )}
        <button
          type="button"
          onClick={() => void exportCsv()}
          disabled={!statement || isExporting}
          className="ml-auto flex items-center gap-1.5 rounded-xl border border-line px-3 py-2 text-xs font-medium text-fg-2 hover:bg-surface-2 disabled:opacity-50"
        >
          <Download className="h-3.5 w-3.5" /> CSV
        </button>
      </div>
      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}
      {statement && (
        <div className="overflow-x-auto rounded-xl border border-line">
          <table className="w-full text-sm">
            <thead className="bg-surface-2 text-left text-xs text-fg-muted">
              <tr>
                <th className="px-3 py-2 font-medium">Date</th>
                <th className="px-3 py-2 font-medium">Details</th>
                <th className="px-3 py-2 text-right font-medium">{debitLabel}</th>
                <th className="px-3 py-2 text-right font-medium">{creditLabel}</th>
                <th className="px-3 py-2 text-right font-medium">Balance</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line tabular-nums">
              <tr className="text-fg-muted">
                <td className="px-3 py-2" colSpan={4}>Opening balance</td>
                <td className="px-3 py-2 text-right">{formatMinor(statement.openingBalanceMinor, currency)}</td>
              </tr>
              {statement.entries.map((entry, index) => (
                <tr key={`${entry.reference}-${index}`}>
                  <td className="whitespace-nowrap px-3 py-2 text-fg-muted">{entry.date.slice(0, 10)}</td>
                  <td className="px-3 py-2">
                    <span className="font-medium text-fg">{entry.reference}</span>
                    <span className="block text-xs text-fg-muted">{entry.description}</span>
                  </td>
                  <td className="px-3 py-2 text-right text-fg">{entry.debitMinor ? formatMinor(entry.debitMinor, currency) : ""}</td>
                  <td className="px-3 py-2 text-right text-success-text">{entry.creditMinor ? formatMinor(entry.creditMinor, currency) : ""}</td>
                  <td className="px-3 py-2 text-right font-semibold text-fg">{formatMinor(entry.balanceMinor, currency)}</td>
                </tr>
              ))}
              {statement.entries.length === 0 && (
                <tr>
                  <td colSpan={5} className="px-3 py-6 text-center text-xs text-fg-muted">No entries in this period.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
