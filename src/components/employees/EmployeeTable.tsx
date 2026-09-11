import { formatMinor } from "../../utils/format";
import type { ManagedEmployee } from "../../types";

interface EmployeeTableProps {
  employees: ManagedEmployee[];
  currency: string;
  isLoading: boolean;
  busyId: number | null;
  onEdit: (employee: ManagedEmployee) => void;
  onToggleActive: (employee: ManagedEmployee) => void;
}

/** Every employee, active or not. Deactivating one is soft — attendance and
 * salary history stay put — so a former employee stays visible here
 * (greyed out) and reactivatable, rather than disappearing entirely. */
export function EmployeeTable({ employees, currency, isLoading, busyId, onEdit, onToggleActive }: EmployeeTableProps) {
  return (
    <div className="overflow-x-auto rounded-lg border border-line bg-surface">
      <table className="min-w-full divide-y divide-line text-sm">
        <thead className="bg-surface-2 text-left text-xs font-medium uppercase tracking-wide text-fg-muted">
          <tr>
            <th className="px-4 py-2">Name</th>
            <th className="px-4 py-2">Role</th>
            <th className="px-4 py-2">Contact</th>
            <th className="px-4 py-2 text-right">Base salary</th>
            <th className="px-4 py-2">Status</th>
            <th className="px-4 py-2" />
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {isLoading ? (
            <tr>
              <td colSpan={6} className="px-4 py-6 text-center text-fg-muted">
                Loading…
              </td>
            </tr>
          ) : employees.length === 0 ? (
            <tr>
              <td colSpan={6} className="px-4 py-6 text-center text-fg-muted">
                No employees yet.
              </td>
            </tr>
          ) : (
            employees.map((employee) => {
              const busy = busyId === employee.id;
              return (
                <tr key={employee.id} className={employee.isActive ? "" : "opacity-60"}>
                  <td className="px-4 py-2 font-medium text-fg">{employee.name}</td>
                  <td className="px-4 py-2 text-fg-2">{employee.role}</td>
                  <td className="px-4 py-2 text-fg-muted">{employee.contact ?? "—"}</td>
                  <td className="px-4 py-2 text-right text-fg-2">
                    {formatMinor(employee.baseSalaryMinor, currency)}
                  </td>
                  <td className="px-4 py-2 text-fg-muted">{employee.isActive ? "Active" : "Deactivated"}</td>
                  <td className="px-4 py-2 text-right">
                    <div className="flex justify-end gap-1.5">
                      <button
                        type="button"
                        onClick={() => onEdit(employee)}
                        disabled={busy}
                        className="rounded-md border border-line-strong px-2.5 py-1 text-xs font-medium text-fg-2 hover:bg-surface-2 disabled:opacity-50"
                      >
                        Edit
                      </button>
                      <button
                        type="button"
                        onClick={() => onToggleActive(employee)}
                        disabled={busy}
                        className="rounded-md border border-line-strong px-2.5 py-1 text-xs font-medium text-fg-2 hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        {employee.isActive ? "Deactivate" : "Reactivate"}
                      </button>
                    </div>
                  </td>
                </tr>
              );
            })
          )}
        </tbody>
      </table>
    </div>
  );
}
