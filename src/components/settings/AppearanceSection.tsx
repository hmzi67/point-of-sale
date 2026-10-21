import { Layers, Moon, Sparkles, Sun } from "lucide-react";
import { useUiPrefsStore } from "../../store";
import type { GlassEffect, Theme } from "../../types";

const OPTIONS: { value: Theme; label: string; hint: string; Icon: typeof Sun }[] = [
  { value: "dark", label: "Dark", hint: "Easier on the eyes for dim counters and long shifts", Icon: Moon },
  { value: "light", label: "Light (Eye-Comfort)", hint: "Warm, glare-free sage & sand palette with calibrated contrast", Icon: Sun },
];

const GLASS_OPTIONS: { value: GlassEffect; label: string; hint: string; Icon: typeof Sparkles }[] = [
  { value: "off", label: "Off", hint: "Solid background surfaces (best for battery & lower-end devices)", Icon: Layers },
  { value: "subtle", label: "Subtle", hint: "Gentle blur on sidebar, top bar, modals & summary cards", Icon: Sparkles },
];

/** Fixed colours for the two theme thumbnails. */
const PREVIEW = {
  dark: { canvas: "#0a0f0d", panel: "#121a17", muted: "#93a39b", faint: "#2a3a33", text: "#c3cec8", card: "#1e2a25", accent: "#00e5a0" },
  light: { canvas: "#f0efea", panel: "#f9f8f5", muted: "#c4c2b7", faint: "#edebe4", text: "#4a524d", card: "#e1efe7", accent: "#2f7d5b" },
} as const;

export function AppearanceSection() {
  const theme = useUiPrefsStore((state) => state.theme);
  const setTheme = useUiPrefsStore((state) => state.setTheme);
  const glassEffect = useUiPrefsStore((state) => state.glassEffect);
  const setGlassEffect = useUiPrefsStore((state) => state.setGlassEffect);

  return (
    <div className="space-y-6">
      <div className="rounded-2xl border border-line bg-surface p-6">
        <h2 className="text-base font-semibold tracking-tight text-fg">Appearance & Theme</h2>
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
                  <rect width="240" height="112" fill={PREVIEW[value].canvas} />
                  <rect width="48" height="112" fill={PREVIEW[value].panel} />
                  <rect x="8" y="12" width="31" height="6" rx="3" fill={PREVIEW[value].muted} />
                  <rect x="6" y="29" width="36" height="13" rx="4" fill={PREVIEW[value].accent} />
                  {[50, 65, 80].map((y) => <rect key={y} x="10" y={y} width="25" height="4" rx="2" fill={PREVIEW[value].faint} />)}
                  <rect x="60" y="14" width="66" height="6" rx="3" fill={PREVIEW[value].text} />
                  {[60, 117, 174].map((x) => <rect key={x} x={x} y="31" width="49" height="27" rx="5" fill={PREVIEW[value].card} />)}
                  <rect x="60" y="67" width="163" height="33" rx="5" fill={PREVIEW[value].panel} />
                  <path d="M70 91 L96 85 L116 88 L141 77 L165 82 L187 73 L213 77" fill="none" stroke={PREVIEW[value].accent} strokeWidth="2" />
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

      <div className="rounded-2xl border border-line bg-surface p-6">
        <h2 className="text-base font-semibold tracking-tight text-fg">Glass Effect</h2>
        <p className="mt-1 text-sm text-fg-muted">
          Subtle translucent blur on navigation, top bar, and dialog overlays. Data surfaces remain completely solid.
        </p>

        <div role="radiogroup" aria-label="Glass effect" className="mt-4 grid gap-3 sm:grid-cols-2">
          {GLASS_OPTIONS.map(({ value, label, hint, Icon }) => {
            const active = glassEffect === value;
            return (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={active}
                onClick={() => setGlassEffect(value)}
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
    </div>
  );
}
