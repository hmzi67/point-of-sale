import { ActiveBadge, PersonAvatar } from "../ui/ManagementSummary";
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
    <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
      <table className="min-w-full divide-y divide-line text-sm">
        <thead className="bg-surface-2/60 text-left text-xs font-semibold text-fg-2">
          <tr>
            <th className="px-4 py-3.5 font-semibold">Name</th>
            <th className="px-4 py-3.5 font-semibold">Role</th>
            <th className="px-4 py-3.5 font-semibold">Contact</th>
            <th className="px-4 py-3.5 text-right font-semibold">Base salary</th>
            <th className="px-4 py-3.5 font-semibold">Status</th>
            <th className="px-4 py-3.5" />
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {isLoading ? (
            <tr>
              <td colSpan={6} className="px-4 py-12 text-center text-fg-muted">
                Loading…
              </td>
            </tr>
          ) : employees.length === 0 ? (
            <tr>
              <td colSpan={6} className="px-4 py-12 text-center text-fg-muted">
                No employees yet.
              </td>
            </tr>
          ) : (
            employees.map((employee) => {
              const busy = busyId === employee.id;
              return (
                <tr key={employee.id} className={employee.isActive ? "transition-colors hover:bg-surface-2/50" : "opacity-60"}>
                  <td className="px-4 py-3.5 font-medium text-fg"><span className="flex items-center gap-3"><PersonAvatar name={employee.name} />{employee.name}</span></td>
                  <td className="px-4 py-3.5 text-fg-2">{employee.role}</td>
                  <td className="px-4 py-3.5 text-fg-muted">{employee.contact ?? "—"}</td>
                  <td className="px-4 py-3.5 whitespace-nowrap text-right font-bold text-fg tabular-nums">
                    {formatMinor(employee.baseSalaryMinor, currency)}
                  </td>
                  <td className="px-4 py-3.5 text-fg-muted"><ActiveBadge active={employee.isActive} /></td>
                  <td className="px-4 py-3.5 text-right">
                    <div className="flex justify-end gap-1.5">
                      <button
                        type="button"
                        onClick={() => onEdit(employee)}
                        disabled={busy}
                        className="rounded-xl border border-line-strong px-3 py-2 text-xs font-medium text-fg-2 hover:bg-surface-2 disabled:opacity-50"
                      >
                        Edit
                      </button>
                      <button
                        type="button"
                        onClick={() => onToggleActive(employee)}
                        disabled={busy}
                        className="rounded-xl border border-line-strong px-3 py-2 text-xs font-medium text-fg-2 hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-50"
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
