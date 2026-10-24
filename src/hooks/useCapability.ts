import { useLicenseStore } from "../store/licenseStore";
import type { Capability } from "../types";

/**
 * Whether the licence's plan includes a capability inside a module (stock
 * takes, advanced reports, extra staff logins), and which plan does when it
 * doesn't. Presentation only: the matching commands refuse in Rust anyway,
 * this just disables the button with an explanation instead of an error.
 */
export function useCapability(capability: Capability) {
  const plan = useLicenseStore((state) => state.plan);
  const allowed = !plan || !plan.restricted || plan.entitlements.includes(capability);
  return {
    allowed,
    requiredPlan: allowed ? null : plan?.upgrade[capability] ?? null,
    planName: plan?.planName ?? null,
  };
}

/** One sentence for a disabled control's tooltip. */
export function upgradeHint(planName: string | null, requiredPlan: string | null): string {
  const current = planName ? `your ${planName} plan` : "your plan";
  return requiredPlan ? `Not part of ${current} — included in ${requiredPlan}` : `Not part of ${current}`;
}
