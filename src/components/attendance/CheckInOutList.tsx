import { useEffect, useState } from "react";
import { CheckCircle2, LogIn, LogOut } from "lucide-react";
import { checkIn, checkOut, getAttendance, getEmployees } from "../../services/attendanceService";
import { formatTime } from "../../utils/format";
import type { AttendanceRecord, Employee } from "../../types";

function todayString(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
}

/** One employee's card state, derived from today's attendance row (if
 * any): no row -> "Check in"; a row with a check-in and no check-out ->
 * "Check out"; both set -> done for today. */
function statusFor(record: AttendanceRecord | undefined): "not-started" | "checked-in" | "done" {
  if (!record || !record.checkIn) return "not-started";
  return record.checkOut ? "done" : "checked-in";
}

/** The daily check-in/check-out screen: one big card per active employee
 * with a single unmissable button — no confirmation step, since speed is
 * the point (a cashier should be able to log arrival in one tap). Whatever
 * today's attendance row already says decides the button: a double-tap
 * updates the same row instead of ever creating a second one (enforced
 * server-side too, see `db::attendance::check_in`/`check_out`). */
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
          <div key={i} className="h-20 animate-pulse rounded-lg border border-slate-200 bg-slate-50" />
        ))}
      </div>
    );
  }

  if (employees.length === 0) {
    return (
      <p className="rounded-lg border border-dashed border-slate-300 px-4 py-10 text-center text-sm text-slate-400">
        No active employees on file. Add one from the Employees screen.
      </p>
    );
  }

  return (
    <div className="space-y-4">
      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {employees.map((employee) => {
          const record = today.get(employee.id);
          const status = statusFor(record);
          const busy = busyId === employee.id;

          return (
            <div
              key={employee.id}
              className="flex flex-col gap-3 rounded-xl border border-slate-200 bg-white p-4 shadow-sm"
            >
              <div>
                <p className="text-base font-semibold text-slate-900">{employee.name}</p>
                <p className="text-sm text-slate-500">{employee.role}</p>
              </div>

              {/* Plain-language time readout — never a raw "HH:MM – HH:MM"
                  pair, which reads as a glitch when both timestamps fall in
                  the same minute during quick real-world use. */}
              {status === "checked-in" && record?.checkIn && (
                <p className="text-sm text-slate-600">Checked in at {formatTime(record.checkIn)}</p>
              )}
              {status === "done" && record?.checkIn && record?.checkOut && (
                <p className="text-sm text-slate-600">
                  Checked in at {formatTime(record.checkIn)}
                  <br />
                  Checked out at {formatTime(record.checkOut)}
                </p>
              )}

              {status === "done" ? (
                <span className="flex items-center justify-center gap-1.5 rounded-lg bg-slate-100 px-4 py-2.5 text-sm font-semibold text-slate-500">
                  <CheckCircle2 className="h-4 w-4" />
                  Done for today
                </span>
              ) : (
                <button
                  type="button"
                  onClick={() => void toggle(employee)}
                  disabled={busy}
                  className={[
                    "flex items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-sm font-semibold text-white transition-colors disabled:cursor-not-allowed disabled:opacity-50",
                    status === "checked-in"
                      ? "bg-orange-600 hover:bg-orange-700"
                      : "bg-emerald-600 hover:bg-emerald-700",
                  ].join(" ")}
                >
                  {status === "checked-in" ? (
                    <>
                      <LogOut className="h-4 w-4" />
                      {busy ? "Checking out…" : "Check Out"}
                    </>
                  ) : (
                    <>
                      <LogIn className="h-4 w-4" />
                      {busy ? "Checking in…" : "Check In"}
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
