import { useState } from "react";
import { AttendanceLogTable } from "../components/attendance/AttendanceLogTable";
import { CheckInOutList } from "../components/attendance/CheckInOutList";
import { MonthlySummaryTable } from "../components/attendance/MonthlySummaryTable";

type Tab = "checkInOut" | "log" | "summary";

const TABS: { key: Tab; label: string; help: string }[] = [
  {
    key: "checkInOut",
    label: "Check in / out",
    help: "Tap Check In when an employee starts their shift, and Check Out when they leave.",
  },
  {
    key: "log",
    label: "Attendance log",
    help: "A day-by-day record of everyone's check-in and check-out times — filter by employee or date range.",
  },
  {
    key: "summary",
    label: "Monthly summary",
    help: "Days present, days absent, and total hours worked per employee for a chosen month.",
  },
];

export function AttendancePage() {
  const [tab, setTab] = useState<Tab>("checkInOut");
  const activeTab = TABS.find((t) => t.key === tab) ?? TABS[0];

  return (
    <section className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold text-fg">Attendance</h2>
        <p className="text-sm text-fg-muted">Daily check-in/out, the shift log, and the monthly summary.</p>
      </div>

      <div className="flex rounded-md border border-line-strong p-0.5 w-fit">
        {TABS.map((t) => (
          <button
            key={t.key}
            type="button"
            onClick={() => setTab(t.key)}
            className={[
              "rounded px-3 py-1.5 text-sm font-medium transition-colors",
              tab === t.key ? "bg-accent text-accent-ink" : "text-fg-2 hover:bg-surface-3",
            ].join(" ")}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* One plain-language line per tab so a non-technical shop owner
       * knows what it's for without guessing. */}
      <p className="rounded-md bg-surface-2 px-3 py-2 text-sm text-fg-2">{activeTab.help}</p>

      {tab === "checkInOut" && <CheckInOutList />}
      {tab === "log" && <AttendanceLogTable />}
      {tab === "summary" && <MonthlySummaryTable />}
    </section>
  );
}
