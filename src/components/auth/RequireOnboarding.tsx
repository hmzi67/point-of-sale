import { Navigate, Outlet } from "react-router-dom";
import { useAppConfig } from "../../hooks/useAppConfig";

/**
 * Gate for the whole normal app: a fresh install (`onboardingCompleted ===
 * false`) is sent to the setup wizard instead of Billing/Dashboard/etc.
 * Sits inside `RequireAuth` — someone still has to log in (the seeded Owner
 * account) before onboarding can run at all.
 *
 * Waits on `isLoading` rather than trusting the placeholder config's default
 * — redirecting off a value that hasn't actually loaded from SQLite yet
 * would risk bouncing a genuinely-configured install back into the wizard
 * for one render.
 */
export function RequireOnboarding() {
  const { config, isLoading } = useAppConfig();
  if (isLoading) return null;
  return config.onboardingCompleted ? <Outlet /> : <Navigate to="/onboarding" replace />;
}
