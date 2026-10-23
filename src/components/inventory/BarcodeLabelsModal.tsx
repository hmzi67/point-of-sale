import { useState } from "react";
import { Barcode } from "lucide-react";
import { generateBarcodes } from "../../services/stockService";
import type { Item } from "../../types";
import { DEFAULT_LABEL_OPTIONS, type LabelOptions } from "../../utils/labelOptions";
import { runAction } from "../../utils/runAction";
import { Field, inputClass, ModalCard, primaryButtonClass, secondaryButtonClass } from "../ui/ModalCard";

const STORAGE_KEY = "diwan.labelOptions";

/** Label layout is a per-device printer setting, so it's remembered on this
 * device only. */
function loadOptions(): LabelOptions {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    return saved ? { ...DEFAULT_LABEL_OPTIONS, ...(JSON.parse(saved) as Partial<LabelOptions>) } : DEFAULT_LABEL_OPTIONS;
  } catch {
    return DEFAULT_LABEL_OPTIONS;
  }
}

/** Generates in-store barcodes for items without one and prints barcode
 * labels (label-printer roll or A4 sticker sheet) as a PDF. */
export function BarcodeLabelsModal({
  items,
  currency,
  onBarcodesGenerated,
  onClose,
}: {
  /** The items to label — the current selection, or everything shown. */
  items: Item[];
  currency: string;
  onBarcodesGenerated: () => void;
  onClose: () => void;
}) {
  const [options, setOptions] = useState<LabelOptions>(loadOptions);
  const [isWorking, setIsWorking] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const missing = items.filter((item) => !item.barcode);
  const printable = items.filter((item) => item.barcode);
  const set = <K extends keyof LabelOptions>(key: K, value: LabelOptions[K]) => setOptions({ ...options, [key]: value });

  const generate = () =>
    runAction(setIsWorking, setMessage, async () => {
      const count = await generateBarcodes(missing.map((item) => item.id));
      setMessage(`${count} barcode(s) generated.`);
      onBarcodesGenerated();
    });

  const print = () =>
    runAction(setIsWorking, setMessage, async () => {
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(options));
      } catch {
        // Remembering the layout is a convenience; printing still works.
      }
      const { downloadLabelsPdf } = await import("../../utils/barcodeLabelsPdf");
      const saved = await downloadLabelsPdf(
        printable.map((item) => ({ name: item.name, barcode: item.barcode as string, priceMinor: item.priceMinor })),
        options,
        currency,
      );
      setMessage(saved ? "Label PDF saved — print it on your label printer." : null);
    });

  return (
    <ModalCard title="Barcodes & labels" subtitle={`${items.length} item(s) · ${printable.length} with a barcode`} width="md" onClose={onClose}>
      <div className="space-y-4">
        {missing.length > 0 && (
          <div className="flex items-center justify-between gap-3 rounded-xl border border-warning-line bg-warning-soft p-3 text-sm text-warning-text">
            <span>{missing.length} item(s) have no barcode yet.</span>
            <button type="button" disabled={isWorking} onClick={() => void generate()} className={`${secondaryButtonClass} flex items-center gap-1.5`}>
              <Barcode className="h-4 w-4" /> Generate
            </button>
          </div>
        )}

        <div className="grid grid-cols-2 gap-1 rounded-xl border border-line p-1">
          {(["roll", "sheet"] as const).map((layout) => (
            <button
              key={layout}
              type="button"
              aria-pressed={options.layout === layout}
              onClick={() => set("layout", layout)}
              className={`rounded-lg py-2 text-xs font-semibold ${options.layout === layout ? "bg-accent text-accent-ink" : "text-fg-muted"}`}
            >
              {layout === "roll" ? "Label printer (roll)" : "A4 sticker sheet"}
            </button>
          ))}
        </div>
        <div className="grid grid-cols-3 gap-3">
          <Field label="Width (mm)">
            <input type="number" min="20" max="120" value={options.widthMm} onChange={(e) => set("widthMm", Number(e.target.value) || 50)} className={inputClass} />
          </Field>
          <Field label="Height (mm)">
            <input type="number" min="15" max="100" value={options.heightMm} onChange={(e) => set("heightMm", Number(e.target.value) || 30)} className={inputClass} />
          </Field>
          <Field label="Copies each">
            <input type="number" min="1" max="500" value={options.copies} onChange={(e) => set("copies", Math.max(1, Number(e.target.value) || 1))} className={inputClass} />
          </Field>
        </div>
        <div className="flex flex-wrap gap-4 text-sm text-fg-2">
          {([["showName", "Product name"], ["showPrice", "Price"], ["showCode", "Code under bars"]] as const).map(([key, label]) => (
            <label key={key} className="flex items-center gap-2">
              <input type="checkbox" checked={options[key]} onChange={(e) => set(key, e.target.checked)} /> {label}
            </label>
          ))}
        </div>

        {message && <p className="text-xs text-fg-muted">{message}</p>}
        <div className="flex justify-end gap-2">
          <button type="button" onClick={onClose} className={secondaryButtonClass}>Close</button>
          <button type="button" disabled={isWorking || printable.length === 0} onClick={() => void print()} className={primaryButtonClass}>
            {isWorking ? "Working…" : `Print ${printable.length * options.copies} label(s)`}
          </button>
        </div>
      </div>
    </ModalCard>
  );
}
