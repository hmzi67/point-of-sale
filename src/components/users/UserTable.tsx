import { ActiveBadge, PersonAvatar } from "../ui/ManagementSummary";
import { roleCanManageAccount } from "../../utils/permissions";
import type { ManagedUser, Role } from "../../types";

const ROLE_LABEL: Record<Role, string> = { owner: "Owner", admin: "Admin", cashier: "Cashier" };
const ROLE_STYLES: Record<Role, string> = {
  // Hue-categorical (see `utils/categoryColor.ts` for why these use raw
  // hues + `dark:` rather than the semantic tokens).
  owner: "bg-violet-500/15 text-violet-700 dark:text-violet-300",
  admin: "bg-sky-500/15 text-sky-700 dark:text-sky-300",
  cashier: "bg-surface-3 text-fg-2",
};

interface UserTableProps {
  users: ManagedUser[];
  currentUserId: number;
  /** The signed-in user's own role — decides which rows' Edit/Deactivate
   * actions are available (mirrors `caller_may_manage` in
   * `src-tauri/src/commands.rs`, which is what actually enforces it). */
  currentUserRole: Role;
  isLoading: boolean;
  busyId: number | null;
  onEdit: (user: ManagedUser) => void;
  onToggleActive: (user: ManagedUser) => void;
}

/** Every staff account, active or not.
 *
 * The Owner account never shows a Deactivate button at all — there is
 * exactly one per installation and it can't be taken offline by anyone,
 * including itself — and its Edit button only works for the Owner's own
 * row (an Admin viewing it sees a disabled Edit with an explanatory
 * tooltip). An Admin viewing a peer Admin's row sees both actions disabled
 * the same way. Every one of these is a courtesy, not the actual guard —
 * `commands::update_user`/`set_user_active` re-check the same hierarchy
 * server-side regardless of what this screen shows. */
export function UserTable({
  users,
  currentUserId,
  currentUserRole,
  isLoading,
  busyId,
  onEdit,
  onToggleActive,
}: UserTableProps) {
  return (
    <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
      <table className="min-w-full divide-y divide-line text-sm">
        <thead className="bg-surface-2/60 text-left text-[10px] font-semibold uppercase tracking-wider text-fg-muted">
          <tr>
            <th className="px-4 py-3.5">Name</th>
            <th className="px-4 py-3.5">Role</th>
            <th className="px-4 py-3.5">Status</th>
            <th className="px-4 py-3.5" />
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {isLoading ? (
            <tr>
              <td colSpan={4} className="px-4 py-12 text-center text-fg-muted">
                Loading…
              </td>
            </tr>
          ) : users.length === 0 ? (
            <tr>
              <td colSpan={4} className="px-4 py-12 text-center text-fg-muted">
                No staff accounts yet.
              </td>
            </tr>
          ) : (
            users.map((user) => {
              const isSelf = user.id === currentUserId;
              const isOwnerRow = user.role === "owner";
              const busy = busyId === user.id;

              const canEdit = roleCanManageAccount(currentUserRole, user.role, isSelf);
              const editTooltip = canEdit
                ? undefined
                : isOwnerRow
                  ? "Only the Owner can edit their own account"
                  : "Only the Owner can manage Admin accounts";

              // The Owner can never be deactivated by anyone — the button
              // doesn't just get disabled here, it isn't rendered at all.
              const canDeactivate = !isOwnerRow && !isSelf && roleCanManageAccount(currentUserRole, user.role, false);
              const deactivateTooltip = isSelf
                ? "You can't deactivate your own account"
                : "Only the Owner can manage Admin accounts";

              return (
                <tr key={user.id} className={user.isActive ? "transition-colors hover:bg-surface-2/50" : "opacity-60"}>
                  <td className="px-4 py-3.5 text-fg-2">
                    <span className="inline-flex items-center gap-3 font-semibold text-fg"><PersonAvatar name={user.name} />{user.name}</span>
                    {isSelf && <span className="ml-1.5 text-xs text-fg-muted">(you)</span>}
                  </td>
                  <td className="px-4 py-3.5">
                    <span className={`rounded-lg px-3 py-2 text-xs font-medium ${ROLE_STYLES[user.role]}`}>
                      {ROLE_LABEL[user.role]}
                    </span>
                  </td>
                  <td className="px-4 py-3.5 text-fg-muted"><ActiveBadge active={user.isActive} /></td>
                  <td className="px-4 py-3.5 text-right">
                    <div className="flex justify-end gap-1.5">
                      <button
                        type="button"
                        onClick={() => onEdit(user)}
                        disabled={busy || !canEdit}
                        title={editTooltip}
                        className="rounded-xl border border-line-strong px-3 py-2 text-xs font-medium text-fg-2 hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-50"
                      >
                        Edit
                      </button>
                      {!isOwnerRow && (
                        <button
                          type="button"
                          onClick={() => onToggleActive(user)}
                          disabled={busy || !canDeactivate}
                          title={canDeactivate ? undefined : deactivateTooltip}
                          className="rounded-xl border border-line-strong px-3 py-2 text-xs font-medium text-fg-2 hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-50"
                        >
                          {user.isActive ? "Deactivate" : "Reactivate"}
                        </button>
                      )}
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
