import { Lock } from "lucide-react";
import { useLicenseStore } from "../../store/licenseStore";

interface UpgradeNoticeProps {
  /** What is locked, e.g. "Purchasing" or "Profit & Loss". */
  feature: string;
  /** The plan that includes it, when the server named one. */
  requiredPlan: string | null;
}

/**
 * Shown in place of a module or feature the licence's plan doesn't include.
 * The plan itself is enforced in Rust; this only explains it. Upgrading is a
 * request to the Diwan team (no online payment), so it points there rather
 * than at a checkout.
 */
export function UpgradeNotice({ feature, requiredPlan }: UpgradeNoticeProps) {
  const planName = useLicenseStore((state) => state.plan?.planName ?? null);
  return (
    <div className="mx-auto mt-10 max-w-lg rounded-2xl border border-line bg-surface p-8 text-center">
      <span className="mx-auto flex h-12 w-12 items-center justify-center rounded-2xl bg-accent-soft text-accent-text">
        <Lock className="h-5 w-5" />
      </span>
      <h2 className="mt-4 text-lg font-semibold text-fg">{feature} isn't part of {planName ? `your ${planName} plan` : "your plan"}</h2>
      <p className="mt-2 text-sm text-fg-muted">
        {requiredPlan ? (
          <>
            It's included in the <strong className="text-fg">{requiredPlan}</strong> plan.{" "}
          </>
        ) : null}
        To upgrade, request the plan from your Diwan account online or contact the Diwan team — there is no online
        payment, and your data and settings carry straight over.
      </p>
    </div>
  );
}
