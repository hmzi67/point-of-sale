import type { ManagedUser, Role } from "../../types";

const ROLE_LABEL: Record<Role, string> = { owner: "Owner", admin: "Admin", cashier: "Cashier" };
const ROLE_STYLES: Record<Role, string> = {
  owner: "bg-violet-50 text-violet-700",
  admin: "bg-blue-50 text-blue-700",
  cashier: "bg-slate-100 text-slate-600",
};

interface UserTableProps {
  users: ManagedUser[];
  currentUserId: number;
  isLoading: boolean;
  busyId: number | null;
  onEdit: (user: ManagedUser) => void;
  onToggleActive: (user: ManagedUser) => void;
}

/** Every staff account, active or not. Deactivating yourself, and
 * deactivating the last active Owner, are both refused server-side
 * (`commands::set_user_active`) — the disabled button here is a courtesy,
 * not the actual guard. */
export function UserTable({ users, currentUserId, isLoading, busyId, onEdit, onToggleActive }: UserTableProps) {
  return (
    <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white">
      <table className="min-w-full divide-y divide-slate-200 text-sm">
        <thead className="bg-slate-50 text-left text-xs font-medium uppercase tracking-wide text-slate-500">
          <tr>
            <th className="px-4 py-2">Name</th>
            <th className="px-4 py-2">Role</th>
            <th className="px-4 py-2">Status</th>
            <th className="px-4 py-2" />
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-100">
          {isLoading ? (
            <tr>
              <td colSpan={4} className="px-4 py-6 text-center text-slate-400">
                Loading…
              </td>
            </tr>
          ) : users.length === 0 ? (
            <tr>
              <td colSpan={4} className="px-4 py-6 text-center text-slate-400">
                No staff accounts yet.
              </td>
            </tr>
          ) : (
            users.map((user) => {
              const isSelf = user.id === currentUserId;
              const busy = busyId === user.id;
              return (
                <tr key={user.id} className={user.isActive ? "" : "opacity-60"}>
                  <td className="px-4 py-2 text-slate-700">
                    {user.name}
                    {isSelf && <span className="ml-1.5 text-xs text-slate-400">(you)</span>}
                  </td>
                  <td className="px-4 py-2">
                    <span className={`rounded px-1.5 py-0.5 text-xs font-medium ${ROLE_STYLES[user.role]}`}>
                      {ROLE_LABEL[user.role]}
                    </span>
                  </td>
                  <td className="px-4 py-2 text-slate-500">{user.isActive ? "Active" : "Deactivated"}</td>
                  <td className="px-4 py-2 text-right">
                    <div className="flex justify-end gap-1.5">
                      <button
                        type="button"
                        onClick={() => onEdit(user)}
                        disabled={busy}
                        className="rounded-md border border-slate-300 px-2.5 py-1 text-xs font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50"
                      >
                        Edit
                      </button>
                      <button
                        type="button"
                        onClick={() => onToggleActive(user)}
                        disabled={busy || (isSelf && user.isActive)}
                        title={isSelf && user.isActive ? "You can't deactivate your own account" : undefined}
                        className="rounded-md border border-slate-300 px-2.5 py-1 text-xs font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        {user.isActive ? "Deactivate" : "Reactivate"}
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
