import { useEffect, useState } from "react";
import { Download, Printer, ReceiptText, Search, X } from "lucide-react";
import { listRecentSales } from "../../services/billingService";
import { createRefund, getSaleForRefund } from "../../services/refundsService";
import { formatMinor } from "../../utils/format";
import type { AppConfig, Refund, RefundableLine, RefundableSale, SaleListItem } from "../../types";

interface RefundModalProps {
  config: AppConfig;
  onClose: () => void;
  /** Called once a refund is successfully created, so the caller can
   * refresh anything that depends on stock (the item grid) or today's
   * totals — the refund itself already restored stock server-side; this is
   * just "go re-fetch what changed". */
  onRefunded: () => void;
}

type Step = "search" | "pick" | "done";

/** Owner/Admin refund flow: search/select the original sale, choose which
 * items/quantities to refund (accounting for any prior partial refund),
 * optional reason, confirm — then offers to print the "Refund Details"
 * receipt. Server-side validation (`refund_create`) is the real guard —
 * this UI's qty caps are a courtesy, not the enforcement. */
export function RefundModal({ config, onClose, onRefunded }: RefundModalProps) {
  const [step, setStep] = useState<Step>("search");

  const [query, setQuery] = useState("");
  const [recentSales, setRecentSales] = useState<SaleListItem[]>([]);
  const [searchError, setSearchError] = useState<string | null>(null);

  const [sale, setSale] = useState<RefundableSale | null>(null);
  const [qtyByLine, setQtyByLine] = useState<Record<number, number>>({});
  const [reason, setReason] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [refund, setRefund] = useState<Refund | null>(null);
  const [printStatus, setPrintStatus] = useState<string | null>(null);

  useEffect(() => {
    listRecentSales(30)
      .then(setRecentSales)
      .catch((e: Error) => setSearchError(e.message));
  }, []);

  const filteredSales = query.trim()
    ? recentSales.filter((s) => String(s.id).includes(query.trim()))
    : recentSales;

  const selectSale = async (saleId: number) => {
    setSearchError(null);
    try {
      const refundable = await getSaleForRefund(saleId);
      setSale(refundable);
      setQtyByLine({});
      setStep("pick");
    } catch (e) {
      setSearchError((e as Error).message);
    }
  };

  const setQty = (line: RefundableLine, qty: number) => {
    const clamped = Math.max(0, Math.min(qty, line.qtyRefundable));
    setQtyByLine((prev) => ({ ...prev, [line.saleItemId]: clamped }));
  };

  const linesToRefund = sale
    ? sale.items
        .map((line) => ({ line, qty: qtyByLine[line.saleItemId] ?? 0 }))
        .filter(({ qty }) => qty > 0)
    : [];
  const totalRefundMinor = linesToRefund.reduce(
    (sum, { line, qty }) => sum + qty * line.priceAtSaleMinor,
    0,
  );

  const submit = async () => {
    if (!sale || linesToRefund.length === 0) return;
    setIsSubmitting(true);
    setError(null);
    try {
      const created = await createRefund(
        sale.saleId,
        linesToRefund.map(({ line, qty }) => ({
          saleItemId: line.saleItemId,
          qty,
          amountMinor: qty * line.priceAtSaleMinor,
        })),
        reason.trim() || null,
      );
      setRefund(created);
      setStep("done");
      onRefunded();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsSubmitting(false);
    }
  };

  const downloadPdf = async () => {
    if (!refund) return;
    try {
      const { downloadRefundPdf } = await import("../../utils/refundPdf");
      await downloadRefundPdf(refund, config);
    } catch (e) {
      setPrintStatus(`Could not prepare the PDF: ${(e as Error).message}`);
    }
  };

  const printThermal = async () => {
    if (!refund) return;
    setPrintStatus(null);
    try {
      const { printRefundThermal } = await import("../../services/refundsService");
      await printRefundThermal(refund.id);
      setPrintStatus("Sent to thermal printer.");
    } catch (e) {
      setPrintStatus((e as Error).message);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="flex max-h-[85dvh] w-full max-w-md flex-col overflow-hidden rounded-3xl bg-white shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-slate-100 px-5 py-3.5">
          <h3 className="flex items-center gap-2 text-sm font-semibold text-slate-900">
            <ReceiptText className="h-4 w-4 text-brand-600" />
            {step === "search" && "Find a Sale to Refund"}
            {step === "pick" && `Refund Sale #${sale?.saleId}`}
            {step === "done" && "Refund Complete"}
          </h3>
          <button type="button" onClick={onClose} className="rounded-full p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="flex-1 space-y-3 overflow-y-auto px-5 py-4">
          {step === "search" && (
            <>
              <div className="relative">
                <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-400" />
                <input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="Search by sale #…"
                  autoFocus
                  className="w-full rounded-2xl border border-slate-200 bg-slate-50 py-2.5 pl-9 pr-3.5 text-sm focus:border-brand-400 focus:outline-none"
                />
              </div>
              {searchError && <p className="text-sm text-red-600">{searchError}</p>}
              <ul className="divide-y divide-slate-100">
                {filteredSales.map((s) => (
                  <li key={s.id}>
                    <button
                      type="button"
                      onClick={() => void selectSale(s.id)}
                      className="flex w-full items-center justify-between gap-3 rounded-xl px-2 py-2.5 text-left text-sm hover:bg-slate-50"
                    >
                      <span>
                        <span className="block font-medium text-slate-900">Sale #{s.id}</span>
                        <span className="block text-xs text-slate-500">
                          {s.createdAt} · {s.cashierName ?? "—"} · {s.paymentMethod}
                        </span>
                      </span>
                      <span className="shrink-0 font-semibold text-slate-900">
                        {formatMinor(s.totalMinor, config.currency)}
                      </span>
                    </button>
                  </li>
                ))}
                {filteredSales.length === 0 && !searchError && (
                  <p className="py-6 text-center text-sm text-slate-400">No matching sales</p>
                )}
              </ul>
            </>
          )}

          {step === "pick" && sale && (
            <>
              <ul className="space-y-2.5">
                {sale.items.map((line) => (
                  <li key={line.saleItemId} className="rounded-2xl bg-slate-50 p-3">
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-sm font-medium text-slate-900">{line.itemName}</span>
                      <span className="text-xs text-slate-500">
                        {formatMinor(line.priceAtSaleMinor, config.currency)} each
                      </span>
                    </div>
                    <div className="mt-1.5 flex items-center justify-between gap-2">
                      <span className="text-xs text-slate-500">
                        {line.qtyRefundable} of {line.qty} refundable
                        {line.qtyAlreadyRefunded > 0 && ` (${line.qtyAlreadyRefunded} already refunded)`}
                      </span>
                      <input
                        type="number"
                        min={0}
                        max={line.qtyRefundable}
                        value={qtyByLine[line.saleItemId] ?? 0}
                        onChange={(e) => setQty(line, Number(e.target.value) || 0)}
                        disabled={line.qtyRefundable === 0}
                        className="w-16 rounded-lg border border-slate-200 bg-white px-2 py-1 text-right text-sm focus:border-brand-400 focus:outline-none disabled:opacity-40"
                      />
                    </div>
                  </li>
                ))}
              </ul>

              <label className="block">
                <span className="mb-1 block text-xs font-medium text-slate-500">Reason (optional)</span>
                <textarea
                  value={reason}
                  onChange={(e) => setReason(e.target.value)}
                  rows={2}
                  placeholder="Why is this being refunded?"
                  className="w-full resize-none rounded-2xl border border-slate-200 bg-slate-50 px-3.5 py-2.5 text-sm placeholder:text-slate-400 focus:border-brand-400 focus:outline-none"
                />
              </label>

              <div className="flex items-center justify-between rounded-2xl bg-brand-50 px-3.5 py-2.5 text-sm font-semibold text-brand-700">
                <span>Total Refund</span>
                <span>{formatMinor(totalRefundMinor, config.currency)}</span>
              </div>

              {error && <p className="text-sm text-red-600">{error}</p>}
            </>
          )}

          {step === "done" && refund && (
            <div className="space-y-2 rounded-2xl bg-emerald-50 p-4 text-center">
              <p className="text-sm font-semibold text-emerald-700">Refund #{refund.id} recorded</p>
              <p className="text-2xl font-bold text-emerald-700">
                {formatMinor(refund.totalRefundAmountMinor, config.currency)}
              </p>
              <p className="text-xs text-emerald-600">Stock has been restored for the refunded quantities.</p>
              {printStatus && <p className="text-xs text-slate-500">{printStatus}</p>}
            </div>
          )}
        </div>

        <div className="space-y-2 border-t border-slate-100 p-5 pt-3">
          {step === "pick" && (
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => setStep("search")}
                className="flex-1 rounded-2xl border border-slate-200 py-2.5 text-sm font-semibold text-slate-600 hover:bg-slate-50"
              >
                Back
              </button>
              <button
                type="button"
                onClick={() => void submit()}
                disabled={linesToRefund.length === 0 || isSubmitting}
                className="flex-1 rounded-2xl bg-brand-600 py-2.5 text-sm font-semibold text-white shadow-soft hover:bg-brand-700 disabled:opacity-50"
              >
                {isSubmitting ? "Refunding…" : "Confirm Refund"}
              </button>
            </div>
          )}

          {step === "done" && (
            <>
              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={() => void downloadPdf()}
                  className="flex flex-1 items-center justify-center gap-1.5 rounded-2xl border border-slate-200 py-2.5 text-sm font-semibold text-slate-700 hover:bg-slate-50"
                >
                  <Download className="h-4 w-4" />
                  PDF
                </button>
                <button
                  type="button"
                  onClick={() => void printThermal()}
                  className="flex flex-1 items-center justify-center gap-1.5 rounded-2xl border border-slate-200 py-2.5 text-sm font-semibold text-slate-700 hover:bg-slate-50"
                >
                  <Printer className="h-4 w-4" />
                  Print
                </button>
              </div>
              <button
                type="button"
                onClick={onClose}
                className="w-full rounded-2xl bg-brand-600 py-2.5 text-sm font-semibold text-white shadow-soft hover:bg-brand-700"
              >
                Done
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
