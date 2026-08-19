import { useState } from "react";
import { AttendanceLogTable } from "../components/attendance/AttendanceLogTable";
import { CheckInOutList } from "../components/attendance/CheckInOutList";
import { MonthlySummaryTable } from "../components/attendance/MonthlySummaryTable";

type Tab = "checkInOut" | "log" | "summary";

const TABS: { key: Tab; label: string }[] = [
  { key: "checkInOut", label: "Check in / out" },
  { key: "log", label: "Attendance log" },
  { key: "summary", label: "Monthly summary" },
];

export function AttendancePage() {
  const [tab, setTab] = useState<Tab>("checkInOut");

  return (
    <section className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold text-slate-900">Attendance</h2>
        <p className="text-sm text-slate-500">Daily check-in/out, the shift log, and the monthly summary.</p>
      </div>

      <div className="flex rounded-md border border-slate-300 p-0.5 w-fit">
        {TABS.map((t) => (
          <button
            key={t.key}
            type="button"
            onClick={() => setTab(t.key)}
            className={[
              "rounded px-3 py-1.5 text-sm font-medium transition-colors",
              tab === t.key ? "bg-brand-600 text-white" : "text-slate-600 hover:bg-slate-100",
            ].join(" ")}
          >
            {t.label}
          </button>
        ))}
      </div>

      {tab === "checkInOut" && <CheckInOutList />}
      {tab === "log" && <AttendanceLogTable />}
      {tab === "summary" && <MonthlySummaryTable />}
    </section>
  );
}
