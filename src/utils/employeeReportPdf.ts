import type { jsPDF } from "jspdf";
import { formatMinor } from "./format";
import { downloadPdf } from "./pdfExport";
import { drawFooter, drawHeader, drawTable, drawTotalsBlock, newReceiptDoc } from "./printLayout";
import type { AppConfig, SalaryCalculation } from "../types";

/** Builds the Employee Report PDF: one row per active employee for `month`
 * — attendance (days present / real days in the month) next to what that
 * attendance produces in salary, plus how much of it has actually been
 * paid and its status — followed by grand totals across every employee.
 * PDF-fallback twin of `printer::escpos::build_employee_report_bytes`,
 * with room (unlike the 42-column thermal ticket) to show every figure —
 * base salary, net pay, paid, status — in one row instead of splitting
 * paid/status onto a second line. */
export function buildEmployeeReportPdf(rows: SalaryCalculation[], month: string, config: AppConfig): jsPDF {
  const estimatedHeightMm = 46 + rows.length * 6 + 26 + (config.receiptFooter.trim() ? 10 : 0);
  const doc = newReceiptDoc(estimatedHeightMm);
  const money = (minor: number) => formatMinor(minor, config.currency);

  let y = drawHeader(doc, config.businessName, ["Employee Report", `Month: ${month}`]);

  y = drawTable(
    doc,
    y,
    ["Employee", "Present", "Base", "Net Pay", "Paid", "Status"],
    rows.map((row) => [
      row.employeeName,
      `${row.daysPresent}/${row.workingDaysInMonth}`,
      money(row.baseSalaryMinor),
      money(row.calculatedAmountMinor),
      money(row.paidAmountMinor),
      statusLabel(row.status),
    ]),
  );

  const totalNetMinor = rows.reduce((sum, row) => sum + row.calculatedAmountMinor, 0);
  const totalPaidMinor = rows.reduce((sum, row) => sum + row.paidAmountMinor, 0);
  y = drawTotalsBlock(
    doc,
    y,
    [["Total Net Pay", money(totalNetMinor)]],
    "Total Paid",
    money(totalPaidMinor),
  );

  drawFooter(doc, y, config.receiptFooter);
  return doc;
}

function statusLabel(status: SalaryCalculation["status"]): string {
  return status === "paid" ? "Paid" : status === "partial" ? "Partial" : "Unpaid";
}

export function downloadEmployeeReportPdf(
  rows: SalaryCalculation[],
  month: string,
  config: AppConfig,
): Promise<boolean> {
  return downloadPdf(buildEmployeeReportPdf(rows, month, config), `employee-report-${month}.pdf`);
}
