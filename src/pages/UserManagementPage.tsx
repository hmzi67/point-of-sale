import { ManagementSummary } from "../components/ui/ManagementSummary";
import { useEffect, useState } from "react";
import { Plus, Users, UserCheck, UserMinus } from "lucide-react";
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
    <section className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-accent-text">Access management</p>
          <h2 className="text-2xl font-bold tracking-tight text-fg sm:text-3xl">User Management</h2>
          <p className="mt-2 max-w-2xl text-sm leading-relaxed text-fg-muted">Create staff accounts, assign roles, and reset PINs.</p>
        </div>
        <button
          type="button"
          onClick={() => setEditingUser("new")}
          className="flex items-center gap-2 rounded-xl bg-accent px-4 py-3 text-sm font-semibold text-accent-ink hover:bg-accent-hover"
        >
          <Plus className="h-4 w-4" />
          Add staff account
        </button>
      </div>

      <ManagementSummary isLoading={isLoading} items={[
        { label: "Staff accounts", value: users.length, detail: "Accounts configured for this store", icon: Users },
        { label: "Active", value: users.filter((person) => person.isActive).length, detail: "Accounts with active access", icon: UserCheck, tone: "success" },
        { label: "Deactivated", value: users.filter((person) => !person.isActive).length, detail: "Accounts with access turned off", icon: UserMinus, tone: "warning" },
      ]} />

      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      <UserTable
        users={users}
        currentUserId={currentUser.id}
        currentUserRole={currentUser.role}
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
