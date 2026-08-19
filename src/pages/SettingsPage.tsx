import { useState } from "react";
import { Lock } from "lucide-react";
import { ToggleSwitch } from "../components/ui/ToggleSwitch";
import { useModuleStore } from "../store";
import { useAppConfig } from "../hooks/useAppConfig";
import { PLATFORM, type ModuleKey } from "../types";

/**
 * The "onboard a new client" screen: flip modules on or off for this
 * installation. No rebuild, no code change — the sidebar and routes follow.
 */
export function SettingsPage() {
  const modules = useModuleStore((state) => state.modules);
  const toggle = useModuleStore((state) => state.toggle);
  const error = useModuleStore((state) => state.error);
  const { config } = useAppConfig();
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
    <section className="mx-auto max-w-3xl space-y-6">
      <div className="rounded-lg border border-slate-200 bg-white p-6">
        <h2 className="text-lg font-semibold text-slate-900">Business</h2>
        <dl className="mt-4 grid grid-cols-2 gap-4 text-sm">
          <div>
            <dt className="text-slate-500">Name</dt>
            <dd className="text-slate-900">{config.businessName}</dd>
          </div>
          <div>
            <dt className="text-slate-500">Type</dt>
            <dd className="capitalize text-slate-900">{config.businessType}</dd>
          </div>
          <div>
            <dt className="text-slate-500">Currency</dt>
            <dd className="text-slate-900">{config.currency}</dd>
          </div>
          <div>
            <dt className="text-slate-500">Tax</dt>
            <dd className="text-slate-900">{config.taxPercent}%</dd>
          </div>
          <div>
            <dt className="text-slate-500">Working days / month</dt>
            <dd className="text-slate-900">{config.workingDaysPerMonth}</dd>
          </div>
        </dl>
      </div>

      <div className="rounded-lg border border-slate-200 bg-white">
        <div className="border-b border-slate-200 p-6">
          <h2 className="text-lg font-semibold text-slate-900">Modules</h2>
          <p className="mt-1 text-sm text-slate-600">
            Choose what this installation shows on{" "}
            <span className="font-medium capitalize">{PLATFORM}</span>. Disabled modules disappear
            from the sidebar and can no longer be opened by URL.
          </p>
        </div>

        {error && (
          <p className="border-b border-red-100 bg-red-50 px-6 py-3 text-sm text-red-700">{error}</p>
        )}

        <ul className="divide-y divide-slate-100">
          {modules.map((module) => (
            <li key={module.key} className="flex items-center justify-between px-6 py-4">
              <div>
                <p className="flex items-center gap-2 text-sm font-medium text-slate-900">
                  {module.name}
                  {module.isCore && (
                    <span className="inline-flex items-center gap-1 rounded bg-slate-100 px-1.5 py-0.5 text-xs font-normal text-slate-500">
                      <Lock className="h-3 w-3" />
                      Core
                    </span>
                  )}
                </p>
                <p className="mt-0.5 text-xs text-slate-500">
                  Android: {module.androidEnabled ? "shown" : "hidden"}
                </p>
              </div>

              <ToggleSwitch
                label={`Toggle ${module.name}`}
                checked={module.enabled}
                disabled={module.isCore || pendingKey === module.key}
                onChange={(enabled) => void onToggle(module.key, enabled)}
              />
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
