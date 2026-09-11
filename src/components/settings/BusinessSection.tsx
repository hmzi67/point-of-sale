import type { AppConfig } from "../../types";

/** Read-only summary of the business basics chosen during onboarding.
 * There is deliberately no editor here yet — changing currency or tax on a
 * live till has receipt/report consequences that need their own flow. */
export function BusinessSection({ config }: { config: AppConfig }) {
  return (
    <div className="rounded-lg border border-line bg-surface p-6">
      <h2 className="text-lg font-semibold text-fg">Business</h2>
      <dl className="mt-4 grid grid-cols-2 gap-4 text-sm sm:grid-cols-3">
        <div>
          <dt className="text-fg-muted">Type</dt>
          <dd className="capitalize text-fg">{config.businessType}</dd>
        </div>
        <div>
          <dt className="text-fg-muted">Currency</dt>
          <dd className="text-fg">{config.currency}</dd>
        </div>
        <div>
          <dt className="text-fg-muted">Tax</dt>
          <dd className="text-fg">{config.taxPercent}%</dd>
        </div>
      </dl>
    </div>
  );
}
