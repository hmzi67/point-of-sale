import { useState, type FormEvent } from "react";
import { X } from "lucide-react";
import { createUser, setUserPin, updateUser } from "../../services/authService";
import { assignableRoles } from "../../utils/permissions";
import type { ManagedUser, Role } from "../../types";

const ROLE_LABEL: Record<Role, string> = { owner: "Owner", admin: "Admin", cashier: "Cashier" };

interface UserFormModalProps {
  /** `null` creates a new account; otherwise edits this one. */
  user: ManagedUser | null;
  /** The signed-in user's own role — decides which roles the dropdown
   * offers (mirrors `commands::create_user`/`update_user`'s
   * `assignable_roles`, which is what actually enforces it; this just keeps
   * the form from offering a choice the server would reject). */
  actingRole: Role;
  onSaved: () => void;
  onClose: () => void;
}

/** Create or edit a staff account: name, role, and — create only, or
 * optionally on edit — a PIN. Editing a PIN is a separate `set_user_pin`
 * call from renaming/re-roling, same split the two Rust functions keep.
 *
 * The Owner account is a special case: there is exactly one per
 * installation, so editing it never offers a role picker at all — its role
 * can't change, only its name and PIN can (this modal is only ever opened
 * on the Owner's row by the Owner themselves; `UserTable` hides Edit on it
 * for everyone else). */
export function UserFormModal({ user, actingRole, onSaved, onClose }: UserFormModalProps) {
  const isEditing = user !== null;
  const isOwnerAccount = user?.role === "owner";
  const [name, setName] = useState(user?.name ?? "");
  const [role, setRole] = useState<Role>(user?.role ?? "cashier");
  const [pin, setPin] = useState("");
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Roles this dropdown may offer: whatever the acting role may assign,
  // plus — when editing — the account's current role, so e.g. an Admin
  // editing their own Admin account doesn't lose their own role as an
  // option just because Admins can't assign it to *others*.
  const selectableRoles = isEditing && user && !assignableRoles(actingRole).includes(user.role)
    ? [user.role, ...assignableRoles(actingRole)]
    : assignableRoles(actingRole);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    if (!isEditing && pin.length < 4) return;

    setIsSaving(true);
    setError(null);
    try {
      if (isEditing) {
        await updateUser(user.id, name.trim(), isOwnerAccount ? "owner" : role);
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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-4">
      <div className="w-full max-w-sm rounded-2xl border border-line bg-surface shadow-xl">
        <div className="flex items-center justify-between border-b border-line px-5 py-4">
          <h3 className="text-sm font-semibold text-fg">
            {isEditing ? `Edit ${user.name}` : "Add staff account"}
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

          {isOwnerAccount ? (
            <div>
              <label className="block text-xs font-medium text-fg-muted">Role</label>
              <p className="mt-1 rounded-xl bg-surface-2 px-3 py-2.5 text-sm text-fg-muted">
                Owner — the installation's single Owner account, can't be changed
              </p>
            </div>
          ) : (
            <div>
              <label className="block text-xs font-medium text-fg-muted">Role</label>
              <select
                value={role}
                onChange={(e) => setRole(e.target.value as Role)}
                className="mt-1 w-full rounded-xl border border-line-strong px-3 py-2.5 text-sm"
              >
                {selectableRoles.map((r) => (
                  <option key={r} value={r}>
                    {ROLE_LABEL[r]}
                  </option>
                ))}
              </select>
            </div>
          )}

          <div>
            <label className="block text-xs font-medium text-fg-muted">
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
              disabled={isSaving || !name.trim() || (!isEditing && pin.length < 4)}
              className="rounded-xl bg-accent px-3 py-1.5 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isSaving ? "Saving…" : isEditing ? "Save changes" : "Create account"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
