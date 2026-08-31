import { X } from "lucide-react";

interface ShortcutsHelpOverlayProps {
  tablesEnabled: boolean;
  onClose: () => void;
}

const SHORTCUTS: { keys: string; description: string; tablesOnly?: boolean }[] = [
  { keys: "0-9 then Enter", description: "Add an item by its quick code (or scan a barcode)" },
  { keys: "T then number, Enter", description: "Jump straight to a table", tablesOnly: true },
  { keys: "↑ / ↓", description: "Move the active cart line" },
  { keys: "← / →", description: "Decrease / increase the active line's quantity" },
  { keys: "Enter (on a line)", description: "For a by-amount item, type its rupee amount" },
  { keys: "Delete or Backspace", description: "Remove the active cart line" },
  { keys: "Ctrl+Enter (⌘+Enter)", description: "Place the order" },
  {
    keys: "Ctrl+K (⌘+K)",
    description: "Print token (KOT) for the current table's order",
    tablesOnly: true,
  },
  { keys: "Esc", description: "Cancel the current quick-entry code or close a popup" },
  { keys: "?", description: "Show or hide this list" },
];

/**
 * Pressing `?` (see `useFastBillingHotkeys`) toggles this — keyboard fast
 * billing is a real change in how the Billing screen behaves, so it needs to
 * be discoverable rather than hidden knowledge a cashier has to be told
 * about in person.
 */
export function ShortcutsHelpOverlay({ tablesEnabled, onClose }: ShortcutsHelpOverlayProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/30" onClick={onClose}>
      <div
        className="w-full max-w-sm rounded-2xl bg-white p-5 shadow-soft-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-slate-900">Keyboard shortcuts</h3>
          <button
            type="button"
            onClick={onClose}
            className="rounded-full p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600"
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <ul className="mt-3 space-y-2">
          {SHORTCUTS.filter((s) => tablesEnabled || !s.tablesOnly).map((shortcut) => (
            <li key={shortcut.keys} className="flex items-center justify-between gap-3 text-sm">
              <kbd className="shrink-0 rounded-lg bg-slate-100 px-2 py-1 text-[11px] font-semibold text-slate-600">
                {shortcut.keys}
              </kbd>
              <span className="text-right text-slate-500">{shortcut.description}</span>
            </li>
          ))}
        </ul>

        <p className="mt-3 text-[11px] text-slate-400">
          Only active while typing isn't focused on a search box or other field.
        </p>
      </div>
    </div>
  );
}
