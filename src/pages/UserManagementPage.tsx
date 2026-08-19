import { useEffect, useState } from "react";
import { Plus } from "lucide-react";
import { UserFormModal } from "../components/users/UserFormModal";
import { UserTable } from "../components/users/UserTable";
import { getAllUsers, setUserActive } from "../services/authService";
import { useAuthStore } from "../store";
import type { ManagedUser } from "../types";

/** Owner/Admin only (gated by `ModuleRoute adminOnly`, same as Settings).
 * Create/edit staff accounts, assign roles, reset PINs, and deactivate
 * accounts — every one of those actions is re-checked server-side against
 * the signed-in session regardless of what this screen shows, so a
 * cashier who somehow reached this URL still couldn't do anything from it. */
export function UserManagementPage() {
  const currentUser = useAuthStore((state) => state.user);
  const [users, setUsers] = useState<ManagedUser[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [editingUser, setEditingUser] = useState<ManagedUser | null | "new">(null);

  const reload = () => {
    setIsLoading(true);
    setError(null);
    getAllUsers()
      .then(setUsers)
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  };

  useEffect(reload, []);

  const toggleActive = async (user: ManagedUser) => {
    setBusyId(user.id);
    setError(null);
    try {
      await setUserActive(user.id, !user.isActive);
      reload();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusyId(null);
    }
  };

  if (!currentUser) return null;

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">User Management</h2>
          <p className="text-sm text-slate-500">Create staff accounts, assign roles, and reset PINs.</p>
        </div>
        <button
          type="button"
          onClick={() => setEditingUser("new")}
          className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50"
        >
          <Plus className="h-4 w-4" />
          Add staff account
        </button>
      </div>

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      <UserTable
        users={users}
        currentUserId={currentUser.id}
        isLoading={isLoading}
        busyId={busyId}
        onEdit={setEditingUser}
        onToggleActive={(user) => void toggleActive(user)}
      />

      {editingUser && (
        <UserFormModal
          user={editingUser === "new" ? null : editingUser}
          actingRole={currentUser.role}
          onClose={() => setEditingUser(null)}
          onSaved={() => {
            setEditingUser(null);
            reload();
          }}
        />
      )}
    </section>
  );
}
