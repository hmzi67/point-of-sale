import { Store, KeyRound, LayoutGrid, Printer, ShieldCheck, SlidersHorizontal } from "lucide-react";
import { useLocation, useNavigate } from "react-router-dom";
import { AppearanceSection } from "../components/settings/AppearanceSection";
import { AreaPinSection } from "../components/settings/AreaPinSection";
import { BusinessSection } from "../components/settings/BusinessSection";
import { CountersSection } from "../components/settings/CountersSection";
import { DeliveryPlatformsSection } from "../components/settings/DeliveryPlatformsSection";
import { LicenseSection } from "../components/settings/LicenseSection";
import { CloudSyncSection } from "../components/settings/CloudSyncSection";
import { ModulesSection } from "../components/settings/ModulesSection";
import { PrinterSettingsSection } from "../components/settings/PrinterSettingsSection";
import { SettingsTabs, type SettingsTab } from "../components/settings/SettingsTabs";
import { StoreIdentitySection } from "../components/settings/StoreIdentitySection";
import { useModuleEnabled } from "../hooks/useModules";
import { useAppConfig } from "../hooks/useAppConfig";

type TabKey = "general" | "printer" | "license" | "security" | "modules";

const TABS: SettingsTab<TabKey>[] = [
  { key: "general", label: "General", icon: SlidersHorizontal, description: "Store details & appearance" },
  { key: "printer", label: "Printer", icon: Printer, description: "Devices & receipt printing" },
  { key: "license", label: "Licence & Sync", icon: ShieldCheck, description: "Licence, cloud sync & web dashboard" },
  { key: "security", label: "Security", icon: KeyRound, description: "Protected areas & PINs" },
  { key: "modules", label: "Modules", icon: LayoutGrid, description: "Features & kitchen counters" },
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
  const tablesEnabled = useModuleEnabled("tables");
  const deliveryEnabled = useModuleEnabled("delivery");

  const active = tabFromHash(hash);
  const selectTab = (key: TabKey) => navigate(`#${key}`, { replace: true });

  return (
    <section className="mx-auto max-w-6xl">
      <header className="mb-6">
        <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-accent-text">Make it yours</p>
        <h2 className="text-2xl font-bold tracking-tight text-fg sm:text-3xl">Settings</h2>
        <p className="mt-2 text-sm text-fg-muted">Manage your store, devices, and preferences in one place.</p>
      </header>

      <div className="grid items-start gap-6 lg:grid-cols-[260px_minmax(0,1fr)]">
        <aside className="min-w-0 space-y-4 lg:sticky lg:top-6">
          <div className="hidden items-center gap-3 rounded-2xl border border-accent/20 bg-linear-to-br from-accent-soft to-surface p-5 lg:flex">
            <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent-text"><Store className="h-5 w-5" /></span>
            <div className="min-w-0"><p className="truncate text-sm font-semibold text-fg" title={config.businessName}>{config.businessName}</p><p className="mt-1 text-xs capitalize text-fg-muted">{config.businessType} · {config.currency}</p></div>
          </div>
          <SettingsTabs tabs={TABS} active={active} onSelect={selectTab} />
        </aside>

      <div className="min-w-0 space-y-5" aria-label={`${TABS.find((tab) => tab.key === active)?.label} settings`}>
        {active === "general" && (
          <>
            <StoreIdentitySection config={config} />
            <BusinessSection config={config} />
            <AppearanceSection />
          </>
        )}

        {active === "printer" && <PrinterSettingsSection config={config} />}

        {active === "license" && (
          <>
            <LicenseSection />
            <CloudSyncSection />
          </>
        )}

        {active === "security" && <AreaPinSection />}

        {active === "modules" && (
          <>
            <ModulesSection />
            {/* KOT tokens are a restaurant/tables workflow, so counter
                management only makes sense — and only appears — when Tables
                is enabled. Sits with the module that switches it on. */}
            {tablesEnabled && <CountersSection />}
            {deliveryEnabled && <DeliveryPlatformsSection />}
          </>
        )}
      </div>
      </div>
    </section>
  );
}
