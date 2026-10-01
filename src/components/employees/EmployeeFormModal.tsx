import { useState, type FormEvent } from "react";
import { X } from "lucide-react";
import { addEmployee, updateEmployee } from "../../services/attendanceService";
import { decimalToMinor, minorToDecimal } from "../../utils/format";
import type { ManagedEmployee } from "../../types";
import { ModalOverlay } from "../ui/ModalOverlay";

/** A short list of common roles for a fast pick — the field stays free text
 * (a `<datalist>`, not a `<select>`) since a shop's actual staff titles
 * won't all fit a fixed list. */
const ROLE_SUGGESTIONS = ["Cashier", "Sales Staff", "Helper", "Supervisor", "Delivery Rider", "Cleaner"];

interface EmployeeFormModalProps {
  /** `null` creates a new employee; otherwise edits this one. */
  employee: ManagedEmployee | null;
  currency: string;
  onSaved: () => void;
  onClose: () => void;
}

/** Create or edit an employee (payroll/attendance record) — name, role,
 * contact number, and base salary. Base salary is entered as a plain
 * currency amount and converted to minor units at submit time, same as
 * price/cost fields in `ItemFormModal`. */
export function EmployeeFormModal({ employee, currency, onSaved, onClose }: EmployeeFormModalProps) {
  const isEditing = employee !== null;
  const [name, setName] = useState(employee?.name ?? "");
  const [role, setRole] = useState(employee?.role ?? "");
  const [contact, setContact] = useState(employee?.contact ?? "");
  const [baseSalary, setBaseSalary] = useState(
    employee ? String(minorToDecimal(employee.baseSalaryMinor)) : "",
  );
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;

    setIsSaving(true);
    setError(null);
    try {
      const salaryMinor = baseSalary.trim() ? decimalToMinor(Number(baseSalary)) : 0;
      if (isEditing) {
        await updateEmployee(employee.id, name.trim(), role, contact.trim() || null, salaryMinor);
      } else {
        await addEmployee(name.trim(), role, contact.trim() || null, salaryMinor);
      }
      onSaved();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <ModalOverlay>
      <div className="w-full max-w-sm rounded-2xl border border-line bg-surface shadow-xl">
        <div className="flex items-center justify-between border-b border-line px-5 py-4">
          <h3 className="text-sm font-semibold text-fg">
            {isEditing ? `Edit ${employee.name}` : "Add employee"}
          </h3>
          <button type="button" onClick={onClose} className="text-fg-muted hover:text-fg-2">
            <X className="h-4 w-4" />
          </button>
        </div>

        <form onSubmit={(e) => void submit(e)} className="space-y-3 px-5 py-4">
          <div>
            <label className="block text-xs font-medium text-fg-muted">Name</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoFocus
              className="mt-1 w-full rounded-xl border border-line-strong px-3 py-2.5 text-sm"
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-fg-muted">Role</label>
            <input
              value={role}
              onChange={(e) => setRole(e.target.value)}
              list="employee-role-suggestions"
              placeholder="e.g. Cashier"
              className="mt-1 w-full rounded-xl border border-line-strong px-3 py-2.5 text-sm"
            />
            <datalist id="employee-role-suggestions">
              {ROLE_SUGGESTIONS.map((r) => (
                <option key={r} value={r} />
              ))}
            </datalist>
          </div>

          <div>
            <label className="block text-xs font-medium text-fg-muted">Contact number</label>
            <input
              value={contact}
              onChange={(e) => setContact(e.target.value)}
              placeholder="Optional"
              className="mt-1 w-full rounded-xl border border-line-strong px-3 py-2.5 text-sm"
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-fg-muted">
              Base salary ({currency}) — used by the Salary module
            </label>
            <input
              type="number"
              min="0"
              step="0.01"
              value={baseSalary}
              onChange={(e) => setBaseSalary(e.target.value)}
              placeholder="0.00"
              className="mt-1 w-full rounded-xl border border-line-strong px-3 py-2.5 text-sm"
            />
          </div>

          {error && <p className="text-xs text-danger-text">{error}</p>}

          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="rounded-xl border border-line-strong px-3 py-1.5 text-sm font-medium text-fg-2 hover:bg-surface-2"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSaving || !name.trim()}
              className="rounded-xl bg-accent px-3 py-1.5 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isSaving ? "Saving…" : isEditing ? "Save changes" : "Add employee"}
            </button>
          </div>
        </form>
      </div>
    </ModalOverlay>
  );
}
