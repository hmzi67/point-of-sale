import { KeyRound, LayoutGrid, Printer, ShieldCheck, SlidersHorizontal } from "lucide-react";
import { useLocation, useNavigate } from "react-router-dom";
import { AppearanceSection } from "../components/settings/AppearanceSection";
import { AreaPinSection } from "../components/settings/AreaPinSection";
import { BusinessSection } from "../components/settings/BusinessSection";
import { CountersSection } from "../components/settings/CountersSection";
import { LicenseSection } from "../components/settings/LicenseSection";
import { ModulesSection } from "../components/settings/ModulesSection";
import { PrinterSettingsSection } from "../components/settings/PrinterSettingsSection";
import { SettingsTabs, type SettingsTab } from "../components/settings/SettingsTabs";
import { StoreIdentitySection } from "../components/settings/StoreIdentitySection";
import { useModuleStore } from "../store";
import { useAppConfig } from "../hooks/useAppConfig";

type TabKey = "general" | "printer" | "license" | "security" | "modules";

const TABS: SettingsTab<TabKey>[] = [
  { key: "general", label: "General", icon: SlidersHorizontal },
  { key: "printer", label: "Printer", icon: Printer },
  { key: "license", label: "Licence", icon: ShieldCheck },
  { key: "security", label: "Security", icon: KeyRound },
  { key: "modules", label: "Modules", icon: LayoutGrid },
];

const DEFAULT_TAB: TabKey = "general";

function tabFromHash(hash: string): TabKey {
  const key = hash.replace(/^#/, "");
  return TABS.some((tab) => tab.key === key) ? (key as TabKey) : DEFAULT_TAB;
}

/**
 * Owner/Admin-only (the route guard, `App.tsx`) — every tab here is the same
 * tier, so there is no per-tab role check. Security (Area PIN) is its own
 * tab on purpose even though it holds one card: a deliberate extra step
 * before touching the thing that can lock staff out of Reports.
 *
 * The active tab *is* the URL hash (`/settings#printer`), not local state.
 * That is what keeps the licence deep link working now that the page is
 * tabbed: `LicenseBanner` and `LicenseLockGate` navigate to
 * `/settings#license`, which lands on the Licence tab, mounts
 * `LicenseSection`, and lets its own `#license` scroll effect run — the
 * section didn't need to learn anything about tabs. It also means the tab
 * survives a re-render and the back button, for free.
 */
export function SettingsPage() {
  const { hash } = useLocation();
  const navigate = useNavigate();
  const { config } = useAppConfig();
  const tablesEnabled = useModuleStore((state) =>
    state.modules.some((module) => module.key === "tables" && module.enabled),
  );

  const active = tabFromHash(hash);
  const selectTab = (key: TabKey) => navigate(`#${key}`, { replace: true });

  return (
    <section className="mx-auto max-w-3xl">
      <header className="mb-4">
        <h2 className="text-lg font-semibold text-fg">Settings</h2>
        <p className="text-sm text-fg-muted">Configure this installation — changes apply immediately.</p>
      </header>

      <SettingsTabs tabs={TABS} active={active} onSelect={selectTab} />

      <div className="mt-6 space-y-6">
        {active === "general" && (
          <>
            <StoreIdentitySection config={config} />
            <BusinessSection config={config} />
            <AppearanceSection />
          </>
        )}

        {active === "printer" && <PrinterSettingsSection config={config} />}

        {active === "license" && <LicenseSection />}

        {active === "security" && <AreaPinSection />}

        {active === "modules" && (
          <>
            <ModulesSection />
            {/* KOT tokens are a restaurant/tables workflow, so counter
                management only makes sense — and only appears — when Tables
                is enabled. Sits with the module that switches it on. */}
            {tablesEnabled && <CountersSection />}
          </>
        )}
      </div>
    </section>
  );
}
