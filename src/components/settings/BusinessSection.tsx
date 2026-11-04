import { useState } from "react";
import { useAppStore, useBillingStore, useModuleStore } from "../../store";
import type { AppConfig, BusinessType } from "../../types";
import { BUSINESS_TYPES } from "../../utils/businessType";
import { runAction } from "../../utils/runAction";

/** The business basics chosen during onboarding. Only the type can be changed
 * here — changing currency or tax on a live till has receipt/report
 * consequences that need their own flow. */
export function BusinessSection({ config }: { config: AppConfig }) {
  const save = useAppStore((state) => state.save);
  const loadModules = useModuleStore((state) => state.load);
  const clearCart = useBillingStore((state) => state.clearCart);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const changeType = (businessType: BusinessType) =>
    runAction(setIsSaving, setError, async () => {
      await save({ businessType });
      // The type decides which modules exist, and a cart in progress may
      // carry a table or delivery details the new type has no screen for.
      await loadModules();
      clearCart();
    });

  return (
    <div className="rounded-2xl border border-line bg-surface p-6">
      <h2 className="text-base font-semibold tracking-tight text-fg">Business</h2>
      <p className="mt-1 text-xs text-fg-muted">Your business setup at a glance.</p>
      <dl className="mt-4 grid grid-cols-1 gap-3 text-sm sm:grid-cols-3">
        <div className="rounded-xl border border-line bg-surface-2/40 p-4">
          <dt className="text-[11px] text-fg-muted">
            <label htmlFor="business-type">Type</label>
          </dt>
          <dd className="mt-2">
            <select
              id="business-type"
              value={config.businessType}
              disabled={isSaving}
              onChange={(e) => void changeType(e.target.value as BusinessType)}
              className="w-full rounded-xl border border-line-strong py-2 pl-3 pr-8 text-sm font-semibold text-fg disabled:opacity-50"
            >
              {BUSINESS_TYPES.map((type) => (
                <option key={type.value} value={type.value}>
                  {type.label}
                </option>
              ))}
            </select>
          </dd>
        </div>
        <div className="rounded-xl border border-line bg-surface-2/40 p-4">
          <dt className="text-[11px] text-fg-muted">Currency</dt>
          <dd className="mt-2 font-semibold text-fg">{config.currency}</dd>
        </div>
        <div className="rounded-xl border border-line bg-surface-2/40 p-4">
          <dt className="text-[11px] text-fg-muted">Tax</dt>
          <dd className="mt-2 font-semibold text-fg">{config.taxPercent}%</dd>
        </div>
      </dl>
      <p className="mt-3 text-xs text-fg-muted">{BUSINESS_TYPES.find((type) => type.value === config.businessType)?.hint}</p>
      {error && <p className="mt-2 rounded-xl bg-danger-soft px-3 py-2 text-xs text-danger-text">{error}</p>}
    </div>
  );
}
