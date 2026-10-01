import { useState } from "react";
import { printShiftSummaryThermal } from "../services/shiftsService";
import type { AppConfig, ShiftSummary } from "../types";

/** The "print on the thermal printer / download as PDF" pair for a shift
 * summary, shared by the close-shift modal and the Shifts history page.
 * `status` is the one-line outcome either action reports. The PDF builder
 * is `import()`-ed on demand so jsPDF never loads with the screen itself. */
export function useShiftSummaryOutput(config: AppConfig) {
  const [status, setStatus] = useState<string | null>(null);

  const printThermal = async (shiftId: number) => {
    setStatus(null);
    try {
      await printShiftSummaryThermal(shiftId);
      setStatus("Sent to thermal printer.");
    } catch (e) {
      setStatus((e as Error).message);
    }
  };

  const downloadPdf = async (summary: ShiftSummary) => {
    try {
      const { downloadShiftSummaryPdf } = await import("../utils/shiftSummaryPdf");
      await downloadShiftSummaryPdf(summary, config);
    } catch (e) {
      setStatus(`Could not prepare the PDF: ${(e as Error).message}`);
    }
  };

  return { status, setStatus, printThermal, downloadPdf };
}
