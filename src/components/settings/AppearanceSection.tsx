import { Moon, Sun } from "lucide-react";
import { useUiPrefsStore } from "../../store";
import type { Theme } from "../../types";

const OPTIONS: { value: Theme; label: string; hint: string; Icon: typeof Sun }[] = [
  { value: "dark", label: "Dark", hint: "Default — easier on the eyes for long shifts", Icon: Moon },
  { value: "light", label: "Light", hint: "Brighter, for well-lit counters", Icon: Sun },
];

/** Theme picker. Also reachable as the sun/moon button in the top bar —
 * this is the discoverable, explained version; that is the quick flip. */
export function AppearanceSection() {
  const theme = useUiPrefsStore((state) => state.theme);
  const setTheme = useUiPrefsStore((state) => state.setTheme);

  return (
    <div className="rounded-2xl border border-line bg-surface p-6">
      <h2 className="text-base font-semibold tracking-tight text-fg">Appearance</h2>
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
                "grid grid-cols-[auto_1fr] items-start gap-3 rounded-xl border p-4 text-left transition-colors",
                active
                  ? "border-accent bg-accent-soft"
                  : "border-line bg-surface-2 hover:border-line-strong hover:bg-surface-3",
              ].join(" ")}
            >
              <svg aria-hidden="true" viewBox="0 0 240 112" className="col-span-2 mb-1 w-full rounded-lg border border-line">
                <rect width="240" height="112" fill={value === "dark" ? "#0b1210" : "#f3f6f4"} />
                <rect width="48" height="112" fill={value === "dark" ? "#17221d" : "#ffffff"} />
                <rect x="8" y="12" width="31" height="6" rx="3" fill={value === "dark" ? "#82978c" : "#c3cfc8"} />
                <rect x="6" y="29" width="36" height="13" rx="4" fill="#00d99a" />
                {[50, 65, 80].map((y) => <rect key={y} x="10" y={y} width="25" height="4" rx="2" fill={value === "dark" ? "#42574b" : "#d5ddd8"} />)}
                <rect x="60" y="14" width="66" height="6" rx="3" fill={value === "dark" ? "#a5b7ad" : "#5b7265"} />
                {[60, 117, 174].map((x) => <rect key={x} x={x} y="31" width="49" height="27" rx="5" fill={value === "dark" ? "#203b2e" : "#d9eee2"} />)}
                <rect x="60" y="67" width="163" height="33" rx="5" fill={value === "dark" ? "#17221d" : "#ffffff"} />
                <path d="M70 91 L96 85 L116 88 L141 77 L165 82 L187 73 L213 77" fill="none" stroke="#00b881" strokeWidth="2" />
              </svg>
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
