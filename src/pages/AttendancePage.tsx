import { CalendarDays, ClipboardList, Clock } from "lucide-react";
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
    <section className="space-y-6">
      <div>
        <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-accent-text">Team attendance</p>
          <h2 className="text-2xl font-bold tracking-tight text-fg sm:text-3xl">Attendance</h2>
        <p className="mt-2 max-w-2xl text-sm leading-relaxed text-fg-muted">Daily check-in/out, the shift log, and the monthly summary.</p>
      </div>

      <div className="flex flex-wrap gap-1 rounded-2xl border border-line bg-surface p-1.5 w-fit">
        {TABS.map((t) => (
          <button
            key={t.key}
            type="button"
            onClick={() => setTab(t.key)}
            aria-pressed={tab === t.key}
            className={[
              "flex items-center gap-2 rounded-lg px-4 py-2.5 text-sm font-medium transition-colors",
              tab === t.key ? "bg-accent text-accent-ink" : "text-fg-2 hover:bg-surface-3",
            ].join(" ")}
          >
            {t.key === "checkInOut" ? <Clock className="h-4 w-4" /> : t.key === "log" ? <ClipboardList className="h-4 w-4" /> : <CalendarDays className="h-4 w-4" />}
            {t.label}
          </button>
        ))}
      </div>

      {/* One plain-language line per tab so a non-technical shop owner
       * knows what it's for without guessing. */}
      <p className="rounded-xl border border-line bg-surface px-4 py-3 text-xs leading-relaxed text-fg-muted">{activeTab.help}</p>

      {tab === "checkInOut" && <CheckInOutList />}
      {tab === "log" && <AttendanceLogTable />}
      {tab === "summary" && <MonthlySummaryTable />}
    </section>
  );
}
