import { ManagementSummary } from "../components/ui/ManagementSummary";
import { useState } from "react";
import { Plus, Users, UserCheck, UserMinus } from "lucide-react";
import { UserFormModal } from "../components/users/UserFormModal";
import { UserTable } from "../components/users/UserTable";
import { useActivatableList } from "../hooks/useActivatableList";
import { upgradeHint, useCapability } from "../hooks/useCapability";
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
  const { items: users, isLoading, error, busyId, reload, toggleActive } = useActivatableList<ManagedUser>(
    getAllUsers,
    setUserActive,
  );
  const [editingUser, setEditingUser] = useState<ManagedUser | null | "new">(null);
  // Extra staff logins are the plan's Multi-User Access feature.
  const multiUser = useCapability("multi_user");

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
          disabled={!multiUser.allowed}
          title={multiUser.allowed ? undefined : upgradeHint(multiUser.planName, multiUser.requiredPlan)}
          className="flex items-center gap-2 rounded-xl bg-accent px-4 py-3 text-sm font-semibold text-accent-ink hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
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
