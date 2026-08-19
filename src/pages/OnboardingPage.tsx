import { useState, type FormEvent } from "react";
import { Navigate, useNavigate } from "react-router-dom";
import { Store } from "lucide-react";
import { useAppConfig } from "../hooks/useAppConfig";
import { useAppStore, useModuleStore } from "../store";
import type { BusinessType } from "../types";

const CURRENCY_SUGGESTIONS = ["PKR", "USD", "INR", "AED", "GBP", "EUR"];

const BUSINESS_TYPES: { value: BusinessType; label: string; hint: string }[] = [
  { value: "retail", label: "Retail shop", hint: "A counter sale business — Table Management stays off." },
  { value: "restaurant", label: "Restaurant / café", hint: "Dine-in tables — Table Management turns on for you." },
  { value: "other", label: "Other", hint: "Start with the retail defaults; adjust modules yourself next." },
];

/**
 * First-time setup, run once per installation (Phase 14). Gated by
 * `RequireOnboarding`: unreachable once `app_config.onboarding_completed`
 * is true, except by navigating here directly, which just bounces back out
 * via the redirect below.
 *
 * Deliberately calls the two commands that already existed
 * (`update_app_config`, `toggle_module`) rather than adding a new one —
 * this is one-time, low-stakes config, not a business write that needs its
 * own atomic transaction the way a sale or a salary payment does.
 */
export function OnboardingPage() {
  const { config } = useAppConfig();
  const save = useAppStore((state) => state.save);
  const toggleModule = useModuleStore((state) => state.toggle);
  const navigate = useNavigate();

  const [businessName, setBusinessName] = useState("");
  const [businessType, setBusinessType] = useState<BusinessType>("retail");
  const [currency, setCurrency] = useState("PKR");
  const [taxPercent, setTaxPercent] = useState("0");
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (config.onboardingCompleted) return <Navigate to="/" replace />;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!businessName.trim() || !currency.trim()) return;

    setIsSaving(true);
    setError(null);
    try {
      await save({
        businessName: businessName.trim(),
        businessType,
        currency: currency.trim().toUpperCase(),
        taxPercent: Number(taxPercent) || 0,
        onboardingCompleted: true,
      });

      // Suggested default, not a hard rule — restaurants get dine-in tables
      // on out of the box; everyone else starts with it off and can flip it
      // on the very next screen if they turn out to need it.
      if (businessType === "restaurant") {
        await toggleModule("tables", true);
      }

      navigate("/settings", { replace: true });
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="flex h-full items-center justify-center overflow-y-auto bg-slate-900 p-6">
      <div className="w-full max-w-lg rounded-xl bg-white p-6 shadow-xl">
        <div className="mb-1 flex items-center gap-2">
          <Store className="h-6 w-6 text-brand-600" />
          <h1 className="text-lg font-semibold text-slate-900">Welcome — let's set up your shop</h1>
        </div>
        <p className="mb-6 text-sm text-slate-500">
          Just a few basics to get started. Everything here can be changed later in Settings.
        </p>

        <form onSubmit={(e) => void submit(e)} className="space-y-5">
          <div>
            <label className="block text-xs font-medium text-slate-500" htmlFor="business-name">
              Business name
            </label>
            <input
              id="business-name"
              value={businessName}
              onChange={(e) => setBusinessName(e.target.value)}
              placeholder="e.g. Al-Noor Store"
              autoFocus
              className="mt-1 w-full rounded-md border border-slate-300 px-3 py-2 text-sm"
            />
          </div>

          <div>
            <span className="block text-xs font-medium text-slate-500">What kind of business is this?</span>
            <div className="mt-2 space-y-2">
              {BUSINESS_TYPES.map((type) => (
                <label
                  key={type.value}
                  className={[
                    "flex cursor-pointer items-start gap-3 rounded-md border p-3 text-sm transition-colors",
                    businessType === type.value ? "border-brand-400 bg-brand-50" : "border-slate-200 hover:bg-slate-50",
                  ].join(" ")}
                >
                  <input
                    type="radio"
                    name="businessType"
                    value={type.value}
                    checked={businessType === type.value}
                    onChange={() => setBusinessType(type.value)}
                    className="mt-0.5"
                  />
                  <span>
                    <span className="block font-medium text-slate-900">{type.label}</span>
                    <span className="text-xs text-slate-500">{type.hint}</span>
                  </span>
                </label>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-slate-500" htmlFor="currency">
                Currency
              </label>
              <input
                id="currency"
                list="currency-suggestions"
                value={currency}
                onChange={(e) => setCurrency(e.target.value)}
                placeholder="PKR"
                className="mt-1 w-full rounded-md border border-slate-300 px-3 py-2 text-sm"
              />
              <datalist id="currency-suggestions">
                {CURRENCY_SUGGESTIONS.map((code) => (
                  <option key={code} value={code} />
                ))}
              </datalist>
            </div>

            <div>
              <label className="block text-xs font-medium text-slate-500" htmlFor="tax-percent">
                Tax rate (%)
              </label>
              <input
                id="tax-percent"
                type="number"
                min={0}
                step="0.01"
                value={taxPercent}
                onChange={(e) => setTaxPercent(e.target.value)}
                className="mt-1 w-full rounded-md border border-slate-300 px-3 py-2 text-sm"
              />
            </div>
          </div>

          {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

          <button
            type="submit"
            disabled={isSaving || !businessName.trim() || !currency.trim()}
            className="w-full rounded-md bg-brand-600 py-2.5 text-sm font-semibold text-white hover:bg-brand-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {isSaving ? "Setting up…" : "Continue to module setup"}
          </button>
        </form>
      </div>
    </div>
  );
}
