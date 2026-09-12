import { useState } from "react";
import { Lock } from "lucide-react";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useModuleStore } from "../../store";
import { PLATFORM, type ModuleKey } from "../../types";

/**
 * The "onboard a new client" card: flip modules on or off for this
 * installation. No rebuild, no code change — the sidebar and routes follow.
 */
export function ModulesSection() {
  const modules = useModuleStore((state) => state.modules);
  const toggle = useModuleStore((state) => state.toggle);
  const error = useModuleStore((state) => state.error);
  const [pendingKey, setPendingKey] = useState<ModuleKey | null>(null);

  const onToggle = async (key: ModuleKey, enabled: boolean) => {
    setPendingKey(key);
    try {
      await toggle(key, enabled);
    } catch {
      // The store keeps the message; nothing to do here.
    } finally {
      setPendingKey(null);
    }
  };

  return (
    <div className="rounded-2xl border border-line bg-surface">
      <div className="border-b border-line bg-surface-2/30 p-6">
        <h2 className="text-base font-semibold tracking-tight text-fg">Modules</h2>
        <p className="mt-1 text-sm text-fg-2">
          Choose what this installation shows on{" "}
          <span className="font-medium capitalize">{PLATFORM}</span>. Disabled modules disappear
          from the sidebar and can no longer be opened by URL.
        </p>
      </div>

      {error && (
        <p className="border-b border-danger-line bg-danger-soft px-6 py-3 text-sm text-danger-text">{error}</p>
      )}

      <ul className="divide-y divide-line">
        {modules.map((module) => (
          <li key={module.key} className="flex items-center justify-between gap-4 px-6 py-5 transition-colors hover:bg-surface-2/40">
            <div>
              <p className="flex items-center gap-2 text-sm font-medium text-fg">
                {module.name}
                {module.isCore && (
                  <span className="inline-flex items-center gap-1 rounded-lg bg-surface-3 px-2 py-1 text-xs font-normal text-fg-muted">
                    <Lock className="h-3 w-3" />
                    Core
                  </span>
                )}
                {module.locked && (
                  <span
                    title="Locked by the product administrator — cannot be changed here"
                    className="inline-flex items-center gap-1 rounded-lg bg-warning-soft px-2 py-1 text-xs font-normal text-warning-text"
                  >
                    <Lock className="h-3 w-3" />
                    Locked
                  </span>
                )}
              </p>
              <p className="mt-0.5 text-xs text-fg-muted">
                Android: {module.androidEnabled ? "shown" : "hidden"}
              </p>
            </div>

            <ToggleSwitch
              label={`Toggle ${module.name}`}
              checked={module.enabled}
              disabled={module.isCore || module.locked || pendingKey === module.key}
              onChange={(enabled) => void onToggle(module.key, enabled)}
            />
          </li>
        ))}
      </ul>
    </div>
  );
}
