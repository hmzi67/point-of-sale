import type { AppConfig } from "../../types";

/** Read-only summary of the business basics chosen during onboarding.
 * There is deliberately no editor here yet — changing currency or tax on a
 * live till has receipt/report consequences that need their own flow. */
export function BusinessSection({ config }: { config: AppConfig }) {
  return (
    <div className="rounded-2xl border border-line bg-surface p-6">
      <h2 className="text-base font-semibold tracking-tight text-fg">Business</h2>
      <p className="mt-1 text-xs text-fg-muted">Your business setup at a glance.</p>
      <dl className="mt-4 grid grid-cols-1 gap-3 text-sm sm:grid-cols-3">
        <div className="rounded-xl border border-line bg-surface-2/40 p-4">
          <dt className="text-[11px] text-fg-muted">Type</dt>
          <dd className="mt-2 font-semibold capitalize text-fg">{config.businessType}</dd>
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
    </div>
  );
}
