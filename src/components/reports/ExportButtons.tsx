import { useState } from "react";
import { Download, FileText } from "lucide-react";
import { downloadReportCsv } from "../../utils/reportCsv";
import type { AppConfig, DailySales, SalesSummary, TopItem } from "../../types";

interface ExportButtonsProps {
  summary: SalesSummary | null;
  topItems: TopItem[];
  series: DailySales[];
  config: AppConfig;
}

export function ExportButtons({ summary, topItems, series, config }: ExportButtonsProps) {
  const [isPreparingPdf, setIsPreparingPdf] = useState(false);
  const disabled = !summary;

  const exportPdf = async () => {
    if (!summary) return;
    setIsPreparingPdf(true);
    try {
      // jsPDF + autoTable is a large chunk with no reason to load until a
      // report is actually exported — deferred the same way the receipt is.
      const { downloadReportPdf } = await import("../../utils/reportPdf");
      downloadReportPdf({ summary, topItems, series, config });
    } finally {
      setIsPreparingPdf(false);
    }
  };

  const exportCsv = () => {
    if (!summary) return;
    downloadReportCsv({ summary, topItems, series, config });
  };

  return (
    <div className="flex gap-2">
      <button
        type="button"
        onClick={() => void exportPdf()}
        disabled={disabled || isPreparingPdf}
        className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
      >
        <FileText className="h-4 w-4" />
        {isPreparingPdf ? "Preparing…" : "Export PDF"}
      </button>
      <button
        type="button"
        onClick={exportCsv}
        disabled={disabled}
        className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
      >
        <Download className="h-4 w-4" />
        Export CSV
      </button>
    </div>
  );
}
