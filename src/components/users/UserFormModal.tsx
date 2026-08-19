import { useState, type FormEvent } from "react";
import { X } from "lucide-react";
import { createUser, setUserPin, updateUser } from "../../services/authService";
import { roleCanAssignRole } from "../../utils/permissions";
import type { ManagedUser, Role } from "../../types";

const ROLE_LABEL: Record<Role, string> = { owner: "Owner", admin: "Admin", cashier: "Cashier" };
const ALL_ROLES: Role[] = ["owner", "admin", "cashier"];

interface UserFormModalProps {
  /** `null` creates a new account; otherwise edits this one. */
  user: ManagedUser | null;
  /** The signed-in user's own role — only an Owner may assign the Owner
   * role to someone (mirrors `commands::create_user`/`update_user`, which is
   * what actually enforces it; this just keeps the form from offering a
   * choice the server would reject). */
  actingRole: Role;
  onSaved: () => void;
  onClose: () => void;
}

/** Create or edit a staff account: name, role, and — create only, or
 * optionally on edit — a PIN. Editing a PIN is a separate `set_user_pin`
 * call from renaming/re-roling, same split the two Rust functions keep. */
export function UserFormModal({ user, actingRole, onSaved, onClose }: UserFormModalProps) {
  const isEditing = user !== null;
  const [name, setName] = useState(user?.name ?? "");
  const [role, setRole] = useState<Role>(user?.role ?? "cashier");
  const [pin, setPin] = useState("");
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    if (!isEditing && pin.length < 4) return;

    setIsSaving(true);
    setError(null);
    try {
      if (isEditing) {
        await updateUser(user.id, name.trim(), role);
        if (pin) await setUserPin(user.id, pin);
      } else {
        await createUser(name.trim(), pin, role);
      }
      onSaved();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="w-full max-w-sm rounded-lg bg-white shadow-xl">
        <div className="flex items-center justify-between border-b border-slate-200 px-5 py-4">
          <h3 className="text-sm font-semibold text-slate-900">
            {isEditing ? `Edit ${user.name}` : "Add staff account"}
          </h3>
          <button type="button" onClick={onClose} className="text-slate-400 hover:text-slate-600">
            <X className="h-4 w-4" />
          </button>
        </div>

        <form onSubmit={(e) => void submit(e)} className="space-y-3 px-5 py-4">
          <div>
            <label className="block text-xs font-medium text-slate-500">Name</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoFocus
              className="mt-1 w-full rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-slate-500">Role</label>
            <select
              value={role}
              onChange={(e) => setRole(e.target.value as Role)}
              className="mt-1 w-full rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
            >
              {ALL_ROLES.map((r) => (
                <option key={r} value={r} disabled={!roleCanAssignRole(actingRole, r)}>
                  {ROLE_LABEL[r]}
                  {!roleCanAssignRole(actingRole, r) ? " (Owner only)" : ""}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-xs font-medium text-slate-500">
              {isEditing ? "Reset PIN (optional)" : "PIN"}
            </label>
            <input
              type="password"
              inputMode="numeric"
              minLength={4}
              maxLength={6}
              value={pin}
              onChange={(e) => setPin(e.target.value.replace(/\D/g, ""))}
              placeholder={isEditing ? "Leave blank to keep current PIN" : "4-6 digits"}
              className="mt-1 w-full rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
            />
          </div>

          {error && <p className="text-xs text-red-600">{error}</p>}

          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-600 hover:bg-slate-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSaving || !name.trim() || (!isEditing && pin.length < 4)}
              className="rounded-md bg-brand-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-brand-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isSaving ? "Saving…" : isEditing ? "Save changes" : "Create account"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
