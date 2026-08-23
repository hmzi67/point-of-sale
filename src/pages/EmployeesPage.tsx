import { useEffect, useState } from "react";
import { Plus } from "lucide-react";
import { EmployeeFormModal } from "../components/employees/EmployeeFormModal";
import { EmployeeTable } from "../components/employees/EmployeeTable";
import { useAppConfig } from "../hooks/useAppConfig";
import { getAllEmployees, setEmployeeActive } from "../services/attendanceService";
import type { ManagedEmployee } from "../types";

/** Owner/Admin only (gated by `ModuleRoute adminOnly`, same as Settings and
 * Users). Add/edit employees and deactivate/reactivate them — these are
 * payroll/attendance records, not login accounts (see `db::attendance`'s
 * module doc comment), so this screen is what the Attendance check-in
 * list and the Salary overview both draw their employee list from. Every
 * action re-checks the signed-in session server-side regardless of what
 * this screen shows. */
export function EmployeesPage() {
  const { config } = useAppConfig();
  const [employees, setEmployees] = useState<ManagedEmployee[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [editingEmployee, setEditingEmployee] = useState<ManagedEmployee | null | "new">(null);

  const reload = () => {
    setIsLoading(true);
    setError(null);
    getAllEmployees()
      .then(setEmployees)
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  };

  useEffect(reload, []);

  const toggleActive = async (employee: ManagedEmployee) => {
    setBusyId(employee.id);
    setError(null);
    try {
      await setEmployeeActive(employee.id, !employee.isActive);
      reload();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusyId(null);
    }
  };

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">Employees</h2>
          <p className="text-sm text-slate-500">
            Add and manage staff records used by Attendance and Salary — deactivate someone who's left instead of
            deleting them, so past attendance and pay history stay intact.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setEditingEmployee("new")}
          className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50"
        >
          <Plus className="h-4 w-4" />
          Add employee
        </button>
      </div>

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      <EmployeeTable
        employees={employees}
        currency={config.currency}
        isLoading={isLoading}
        busyId={busyId}
        onEdit={setEditingEmployee}
        onToggleActive={(employee) => void toggleActive(employee)}
      />

      {editingEmployee && (
        <EmployeeFormModal
          employee={editingEmployee === "new" ? null : editingEmployee}
          currency={config.currency}
          onClose={() => setEditingEmployee(null)}
          onSaved={() => {
            setEditingEmployee(null);
            reload();
          }}
        />
      )}
    </section>
  );
}
