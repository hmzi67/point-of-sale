import { Moon, Sun } from "lucide-react";
import { useThemeStore } from "../../store";
import type { Theme } from "../../types";

const OPTIONS: { value: Theme; label: string; hint: string; Icon: typeof Sun }[] = [
  { value: "dark", label: "Dark", hint: "Default — easier on the eyes for long shifts", Icon: Moon },
  { value: "light", label: "Light", hint: "Brighter, for well-lit counters", Icon: Sun },
];

/** Theme picker. Also reachable as the sun/moon button in the top bar —
 * this is the discoverable, explained version; that is the quick flip. */
export function AppearanceSection() {
  const theme = useThemeStore((state) => state.theme);
  const setTheme = useThemeStore((state) => state.setTheme);

  return (
    <div className="rounded-lg border border-line bg-surface p-6">
      <h2 className="text-lg font-semibold text-fg">Appearance</h2>
      <p className="mt-1 text-sm text-fg-muted">
        Applies to this device and is remembered across restarts.
      </p>

      <div role="radiogroup" aria-label="Theme" className="mt-4 grid gap-3 sm:grid-cols-2">
        {OPTIONS.map(({ value, label, hint, Icon }) => {
          const active = theme === value;
          return (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={active}
              onClick={() => void setTheme(value)}
              className={[
                "flex items-start gap-3 rounded-xl border p-4 text-left transition-colors",
                active
                  ? "border-accent bg-accent-soft"
                  : "border-line bg-surface-2 hover:border-line-strong hover:bg-surface-3",
              ].join(" ")}
            >
              <span
                className={[
                  "flex h-9 w-9 shrink-0 items-center justify-center rounded-lg",
                  active ? "bg-accent text-accent-ink" : "bg-surface-3 text-fg-muted",
                ].join(" ")}
              >
                <Icon className="h-4.5 w-4.5" />
              </span>
              <span>
                <span className={`block text-sm font-semibold ${active ? "text-accent-text" : "text-fg"}`}>
                  {label}
                </span>
                <span className="mt-0.5 block text-xs text-fg-muted">{hint}</span>
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
