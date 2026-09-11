import type { LucideIcon } from "lucide-react";

export interface SettingsTab<K extends string> {
  key: K;
  label: string;
  icon: LucideIcon;
}

interface SettingsTabsProps<K extends string> {
  tabs: SettingsTab<K>[];
  active: K;
  onSelect: (key: K) => void;
}

/**
 * The horizontal tab strip under the Settings header. Purely presentational
 * — which tab is active, and what selecting one does, is the page's
 * business (it keeps the choice in the URL hash, see `SettingsPage`).
 *
 * `overflow-x-auto` + `shrink-0` tabs: desktop's window has a 1024px
 * minimum so this never matters there, but Android has none, and a strip
 * that scrolls beats one that wraps into two ragged rows on a phone.
 */
export function SettingsTabs<K extends string>({ tabs, active, onSelect }: SettingsTabsProps<K>) {
  return (
    <nav aria-label="Settings sections" className="border-b border-line">
      <ul role="tablist" className="-mb-px flex gap-1 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
        {tabs.map(({ key, label, icon: Icon }) => {
          const isActive = key === active;
          return (
            <li key={key} role="presentation" className="shrink-0">
              <button
                type="button"
                role="tab"
                aria-selected={isActive}
                onClick={() => onSelect(key)}
                className={[
                  "flex items-center gap-2 border-b-2 px-3 py-3 text-sm font-medium transition-colors",
                  isActive
                    ? "border-accent text-accent-text"
                    : "border-transparent text-fg-muted hover:border-line-strong hover:text-fg",
                ].join(" ")}
              >
                <Icon className="h-4 w-4" />
                {label}
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
