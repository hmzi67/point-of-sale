import { useEffect, useState } from "react";
import { useAppStore } from "../../store";
import type { AppConfig } from "../../types";
import { runAction } from "../../utils/runAction";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { inputClass } from "../ui/ModalCard";

/** Checkout charges and weight-scale barcodes. Tax and service charge can
 * still be switched off for a single sale from the cart. */
export function CheckoutSettingsSection({ config }: { config: AppConfig }) {
  const save = useAppStore((state) => state.save);
  const [service, setService] = useState(String(config.serviceChargePercent));
  const [scale, setScale] = useState({
    prefix: config.scaleBarcodePrefix,
    itemDigits: String(config.scaleItemCodeDigits),
    kind: config.scaleValueKind,
    decimals: String(config.scaleValueDecimals),
  });
  const [isSaving, setIsSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => setService(String(config.serviceChargePercent)), [config.serviceChargePercent]);

  const persist = (patch: Partial<AppConfig>) =>
    runAction(setIsSaving, setMessage, async () => {
      await save(patch);
      setMessage("Saved.");
    });

  const valueDigits = 12 - scale.prefix.length - Number(scale.itemDigits || 0);
  const example = `${scale.prefix}${"1".padStart(Number(scale.itemDigits) || 0, "0")}${"0".repeat(Math.max(0, valueDigits))}C`;

  return (
    <div className="rounded-2xl border border-line bg-surface p-6">
      <h2 className="text-base font-semibold tracking-tight text-fg">Checkout</h2>
      <p className="mt-1 text-xs text-fg-muted">
        Tax is {config.taxPercent}% (set during setup). Cashiers can remove tax or the service charge for a single sale from the cart.
      </p>

      <div className="mt-5 flex flex-wrap items-end gap-3">
        <label className="block">
          <span className="block text-xs font-medium text-fg-muted">Service charge (%)</span>
          <input
            type="number"
            min="0"
            max="100"
            step="0.5"
            value={service}
            onChange={(e) => setService(e.target.value)}
            className="mt-1 w-32 rounded-xl border border-line-strong px-3 py-2.5 text-sm"
          />
        </label>
        <button
          type="button"
          disabled={isSaving || Number(service) === config.serviceChargePercent}
          onClick={() => void persist({ serviceChargePercent: Number(service) || 0 })}
          className="rounded-xl bg-accent px-3.5 py-2.5 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:opacity-50"
        >
          Save
        </button>
        <span className="pb-2.5 text-xs text-fg-muted">0 turns it off.</span>
      </div>

      <div className="mt-6 border-t border-line pt-5">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h3 className="text-sm font-semibold text-fg">Weight-scale barcodes</h3>
            <p className="mt-0.5 text-xs text-fg-muted">
              Read price/weight labels printed by a shop scale. The item code is matched to the item's short code.
            </p>
          </div>
          <ToggleSwitch
            checked={config.scaleBarcodeEnabled}
            label="Weight-scale barcodes"
            disabled={isSaving}
            onChange={(checked) => void persist({ scaleBarcodeEnabled: checked })}
          />
        </div>
        {config.scaleBarcodeEnabled && (
          <div className="mt-4 space-y-3">
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              <label className="block">
                <span className="block text-xs font-medium text-fg-muted">Prefix</span>
                <input value={scale.prefix} onChange={(e) => setScale({ ...scale, prefix: e.target.value.replace(/\D/g, "").slice(0, 3) })} className={`mt-1 ${inputClass}`} />
              </label>
              <label className="block">
                <span className="block text-xs font-medium text-fg-muted">Item code digits</span>
                <input type="number" min="3" max="6" value={scale.itemDigits} onChange={(e) => setScale({ ...scale, itemDigits: e.target.value })} className={`mt-1 ${inputClass}`} />
              </label>
              <label className="block">
                <span className="block text-xs font-medium text-fg-muted">Embedded value</span>
                <select value={scale.kind} onChange={(e) => setScale({ ...scale, kind: e.target.value as "weight" | "price" })} className="mt-1 w-full rounded-xl border border-line-strong py-2.5 pl-3 pr-8 text-sm">
                  <option value="weight">Weight</option>
                  <option value="price">Price</option>
                </select>
              </label>
              <label className="block">
                <span className="block text-xs font-medium text-fg-muted">Decimals</span>
                <input type="number" min="0" max="3" value={scale.decimals} onChange={(e) => setScale({ ...scale, decimals: e.target.value })} className={`mt-1 ${inputClass}`} />
              </label>
            </div>
            <p className="font-mono text-xs text-fg-muted">
              Layout: {example} — prefix, item code, {Math.max(0, valueDigits)}-digit {scale.kind} ({scale.decimals} decimals), check digit.
            </p>
            <button
              type="button"
              disabled={isSaving || valueDigits < 1}
              onClick={() =>
                void persist({
                  scaleBarcodePrefix: scale.prefix,
                  scaleItemCodeDigits: Number(scale.itemDigits),
                  scaleValueKind: scale.kind,
                  scaleValueDecimals: Number(scale.decimals),
                })
              }
              className="rounded-xl bg-accent px-3.5 py-2 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:opacity-50"
            >
              Save scale layout
            </button>
          </div>
        )}
      </div>
      {message && <p className="mt-3 text-xs text-fg-muted">{message}</p>}
    </div>
  );
}
