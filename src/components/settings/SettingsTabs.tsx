import { ChevronRight, type LucideIcon } from "lucide-react";

export interface SettingsTab<K extends string> {
  key: K;
  label: string;
  icon: LucideIcon;
  description?: string;
}

interface SettingsTabsProps<K extends string> {
  tabs: SettingsTab<K>[];
  active: K;
  onSelect: (key: K) => void;
}

/** Settings navigation uses the page's URL hash and adapts to a mobile strip. */
export function SettingsTabs<K extends string>({ tabs, active, onSelect }: SettingsTabsProps<K>) {
  return (
    <nav aria-label="Settings sections" className="rounded-2xl border border-line bg-surface p-2">
      <p className="hidden px-3 pb-3 pt-3 text-[10px] font-semibold uppercase tracking-widest text-fg-muted lg:block">Preferences</p>
      <ul className="flex gap-1 overflow-x-auto lg:flex-col">
        {tabs.map(({ key, label, description, icon: Icon }) => {
          const isActive = key === active;
          return (
            <li key={key} className="shrink-0">
              <button type="button" aria-current={isActive ? "page" : undefined} onClick={() => onSelect(key)}
                className={`flex w-full items-center gap-3 rounded-xl border px-3 py-3 text-left transition-colors ${isActive ? "border-accent/25 bg-accent-soft text-accent-text" : "border-transparent text-fg-muted hover:bg-surface-2 hover:text-fg"}`}>
                <span className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-lg ${isActive ? "bg-accent text-accent-ink" : "bg-surface-3"}`}><Icon className="h-4 w-4" /></span>
                <span className="flex-1">
                  <span className="block text-xs font-semibold">{label}</span>
                  <span className="mt-1 hidden text-[10px] text-fg-muted lg:block">{description}</span>
                </span>
                <ChevronRight className="hidden h-3.5 w-3.5 shrink-0 lg:block" />
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
