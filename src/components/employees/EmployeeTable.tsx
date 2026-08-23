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
    <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white">
      <table className="min-w-full divide-y divide-slate-200 text-sm">
        <thead className="bg-slate-50 text-left text-xs font-medium uppercase tracking-wide text-slate-500">
          <tr>
            <th className="px-4 py-2">Name</th>
            <th className="px-4 py-2">Role</th>
            <th className="px-4 py-2">Contact</th>
            <th className="px-4 py-2 text-right">Base salary</th>
            <th className="px-4 py-2">Status</th>
            <th className="px-4 py-2" />
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-100">
          {isLoading ? (
            <tr>
              <td colSpan={6} className="px-4 py-6 text-center text-slate-400">
                Loading…
              </td>
            </tr>
          ) : employees.length === 0 ? (
            <tr>
              <td colSpan={6} className="px-4 py-6 text-center text-slate-400">
                No employees yet.
              </td>
            </tr>
          ) : (
            employees.map((employee) => {
              const busy = busyId === employee.id;
              return (
                <tr key={employee.id} className={employee.isActive ? "" : "opacity-60"}>
                  <td className="px-4 py-2 font-medium text-slate-900">{employee.name}</td>
                  <td className="px-4 py-2 text-slate-600">{employee.role}</td>
                  <td className="px-4 py-2 text-slate-500">{employee.contact ?? "—"}</td>
                  <td className="px-4 py-2 text-right text-slate-700">
                    {formatMinor(employee.baseSalaryMinor, currency)}
                  </td>
                  <td className="px-4 py-2 text-slate-500">{employee.isActive ? "Active" : "Deactivated"}</td>
                  <td className="px-4 py-2 text-right">
                    <div className="flex justify-end gap-1.5">
                      <button
                        type="button"
                        onClick={() => onEdit(employee)}
                        disabled={busy}
                        className="rounded-md border border-slate-300 px-2.5 py-1 text-xs font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50"
                      >
                        Edit
                      </button>
                      <button
                        type="button"
                        onClick={() => onToggleActive(employee)}
                        disabled={busy}
                        className="rounded-md border border-slate-300 px-2.5 py-1 text-xs font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
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
