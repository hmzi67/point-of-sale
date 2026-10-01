import { ManagementSummary } from "../components/ui/ManagementSummary";
import { useState } from "react";
import { Plus, Users, UserCheck, UserMinus } from "lucide-react";
import { EmployeeFormModal } from "../components/employees/EmployeeFormModal";
import { EmployeeTable } from "../components/employees/EmployeeTable";
import { useActivatableList } from "../hooks/useActivatableList";
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
  const { items: employees, isLoading, error, busyId, reload, toggleActive } = useActivatableList<ManagedEmployee>(
    getAllEmployees,
    setEmployeeActive,
  );
  const [editingEmployee, setEditingEmployee] = useState<ManagedEmployee | null | "new">(null);

  return (
    <section className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-accent-text">Your team</p>
          <h2 className="text-2xl font-bold tracking-tight text-fg sm:text-3xl">Employees</h2>
          <p className="mt-2 max-w-2xl text-sm leading-relaxed text-fg-muted">
            Add and manage staff records used by Attendance and Salary — deactivate someone who's left instead of
            deleting them, so past attendance and pay history stay intact.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setEditingEmployee("new")}
          className="flex items-center gap-2 rounded-xl bg-accent px-4 py-3 text-sm font-semibold text-accent-ink hover:bg-accent-hover"
        >
          <Plus className="h-4 w-4" />
          Add employee
        </button>
      </div>

      <ManagementSummary isLoading={isLoading} items={[
        { label: "Total employees", value: employees.length, detail: "Attendance and payroll records", icon: Users },
        { label: "Active", value: employees.filter((person) => person.isActive).length, detail: "Current members of your team", icon: UserCheck, tone: "success" },
        { label: "Deactivated", value: employees.filter((person) => !person.isActive).length, detail: "History retained for your records", icon: UserMinus, tone: "warning" },
      ]} />

      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

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
