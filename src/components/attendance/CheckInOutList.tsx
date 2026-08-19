import { useEffect, useState } from "react";
import { LogIn, LogOut } from "lucide-react";
import { checkIn, checkOut, getAttendance, getEmployees } from "../../services/attendanceService";
import { formatTime } from "../../utils/format";
import type { AttendanceRecord, Employee } from "../../types";

function todayString(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
}

/** One employee's button state, derived from today's attendance row (if
 * any): no row -> "Check in"; a row with a check-in and no check-out ->
 * "Check out"; both set -> done for today. */
function statusFor(record: AttendanceRecord | undefined): "not-started" | "checked-in" | "done" {
  if (!record || !record.checkIn) return "not-started";
  return record.checkOut ? "done" : "checked-in";
}

/** The daily check-in/check-out screen: one row per active employee, a
 * single button that toggles between "Check in" and "Check out" based on
 * whatever today's attendance row already says — so a double-tap updates the
 * same row instead of ever creating a second one (enforced server-side too,
 * see `db::attendance::check_in`/`check_out`). */
export function CheckInOutList() {
  const [employees, setEmployees] = useState<Employee[]>([]);
  const [today, setToday] = useState<Map<number, AttendanceRecord>>(new Map());
  const [isLoading, setIsLoading] = useState(true);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = () => {
    setIsLoading(true);
    setError(null);
    const date = todayString();
    Promise.all([getEmployees(), getAttendance(null, date, date)])
      .then(([staff, records]) => {
        setEmployees(staff);
        setToday(new Map(records.map((r) => [r.employeeId, r])));
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  };

  useEffect(reload, []);

  const toggle = async (employee: Employee) => {
    const status = statusFor(today.get(employee.id));
    setBusyId(employee.id);
    setError(null);
    try {
      const record = status === "checked-in" ? await checkOut(employee.id) : await checkIn(employee.id);
      setToday((prev) => new Map(prev).set(employee.id, record));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusyId(null);
    }
  };

  if (isLoading) {
    return (
      <div className="space-y-2">
        {Array.from({ length: 3 }).map((_, i) => (
          <div key={i} className="h-16 animate-pulse rounded-lg border border-slate-200 bg-slate-50" />
        ))}
      </div>
    );
  }

  if (employees.length === 0) {
    return (
      <p className="rounded-lg border border-dashed border-slate-300 px-4 py-10 text-center text-sm text-slate-400">
        No active employees on file.
      </p>
    );
  }

  return (
    <div className="space-y-4">
      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      <div className="divide-y divide-slate-100 rounded-lg border border-slate-200 bg-white">
        {employees.map((employee) => {
          const record = today.get(employee.id);
          const status = statusFor(record);
          const busy = busyId === employee.id;

          return (
            <div key={employee.id} className="flex items-center justify-between gap-3 px-4 py-3">
              <div>
                <p className="text-sm font-medium text-slate-900">{employee.name}</p>
                <p className="text-xs text-slate-500">
                  {employee.role}
                  {status === "checked-in" && record?.checkIn && ` · in since ${formatTime(record.checkIn)}`}
                  {status === "done" && record?.checkIn && record?.checkOut && (
                    <> · {formatTime(record.checkIn)} – {formatTime(record.checkOut)}</>
                  )}
                </p>
              </div>

              {status === "done" ? (
                <span className="rounded-md bg-emerald-50 px-3 py-1.5 text-xs font-medium text-emerald-700">
                  Done for today
                </span>
              ) : (
                <button
                  type="button"
                  onClick={() => void toggle(employee)}
                  disabled={busy}
                  className={[
                    "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-semibold text-white transition-colors disabled:cursor-not-allowed disabled:opacity-50",
                    status === "checked-in" ? "bg-amber-600 hover:bg-amber-700" : "bg-brand-600 hover:bg-brand-700",
                  ].join(" ")}
                >
                  {status === "checked-in" ? (
                    <>
                      <LogOut className="h-3.5 w-3.5" />
                      {busy ? "Checking out…" : "Check out"}
                    </>
                  ) : (
                    <>
                      <LogIn className="h-3.5 w-3.5" />
                      {busy ? "Checking in…" : "Check in"}
                    </>
                  )}
                </button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
