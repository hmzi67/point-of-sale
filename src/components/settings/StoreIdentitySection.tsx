import { useEffect, useRef, useState } from "react";
import { ImageOff, ImagePlus, Loader2 } from "lucide-react";
import { useLogoDataUrl } from "../../hooks/useLogoDataUrl";
import { useAppStore } from "../../store";
import { readLogoFile } from "../../utils/image";
import type { AppConfig } from "../../types";

interface StoreIdentitySectionProps {
  config: AppConfig;
}

/**
 * Business name + logo, editable post-setup (Phase 14's onboarding wizard
 * is still the *first* time these are set; this is how they change after).
 * Owner/Admin only — same gate as the rest of Settings, enforced server-side
 * by `update_app_config`/`config_upload_logo` regardless of what this
 * screen shows.
 */
export function StoreIdentitySection({ config }: StoreIdentitySectionProps) {
  const save = useAppStore((state) => state.save);
  const uploadLogo = useAppStore((state) => state.uploadLogo);

  // Local draft so typing doesn't write to the store (and re-render every
  // other Settings consumer) on every keystroke — only on blur, when it
  // actually differs from what's saved.
  const [nameDraft, setNameDraft] = useState(config.businessName);
  const [isSavingName, setIsSavingName] = useState(false);
  const [nameError, setNameError] = useState<string | null>(null);

  useEffect(() => setNameDraft(config.businessName), [config.businessName]);

  const [phoneDraft, setPhoneDraft] = useState(config.phone ?? "");
  const [isSavingPhone, setIsSavingPhone] = useState(false);
  const [phoneError, setPhoneError] = useState<string | null>(null);

  useEffect(() => setPhoneDraft(config.phone ?? ""), [config.phone]);

  const [deliveryDraft, setDeliveryDraft] = useState(config.deliveryNumber ?? "");
  const [isSavingDelivery, setIsSavingDelivery] = useState(false);
  const [deliveryError, setDeliveryError] = useState<string | null>(null);

  useEffect(() => setDeliveryDraft(config.deliveryNumber ?? ""), [config.deliveryNumber]);

  const logoDataUrl = useLogoDataUrl(config.logoPath);
  const [isUploadingLogo, setIsUploadingLogo] = useState(false);
  const [logoError, setLogoError] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const saveNameIfChanged = async () => {
    const trimmed = nameDraft.trim();
    if (!trimmed || trimmed === config.businessName) {
      setNameDraft(config.businessName); // revert an empty/unchanged edit
      return;
    }
    setIsSavingName(true);
    setNameError(null);
    try {
      await save({ businessName: trimmed });
    } catch (e) {
      setNameError((e as Error).message);
      setNameDraft(config.businessName);
    } finally {
      setIsSavingName(false);
    }
  };

  const savePhoneIfChanged = async () => {
    const trimmed = phoneDraft.trim();
    const current = config.phone ?? "";
    if (trimmed === current) {
      setPhoneDraft(current);
      return;
    }
    setIsSavingPhone(true);
    setPhoneError(null);
    try {
      // An empty value is saved as "" (not omitted) so clearing a
      // previously-set number actually clears it — `AppConfigUpdate`'s
      // COALESCE-based patch treats `null`/omitted as "leave as-is", so
      // there's no other way to unset this field once set.
      await save({ phone: trimmed });
    } catch (e) {
      setPhoneError((e as Error).message);
      setPhoneDraft(current);
    } finally {
      setIsSavingPhone(false);
    }
  };

  const saveDeliveryIfChanged = async () => {
    const trimmed = deliveryDraft.trim();
    const current = config.deliveryNumber ?? "";
    if (trimmed === current) {
      setDeliveryDraft(current);
      return;
    }
    setIsSavingDelivery(true);
    setDeliveryError(null);
    try {
      // Empty is saved as "" (not omitted) so clearing it actually clears
      // it — same reasoning as `savePhoneIfChanged` above.
      await save({ deliveryNumber: trimmed });
    } catch (e) {
      setDeliveryError((e as Error).message);
      setDeliveryDraft(current);
    } finally {
      setIsSavingDelivery(false);
    }
  };

  const handleLogoSelected = async (file: File | undefined) => {
    if (!file) return;
    setLogoError(null);
    setIsUploadingLogo(true);
    try {
      const { base64, extension } = await readLogoFile(file);
      await uploadLogo(base64, extension);
    } catch (e) {
      setLogoError((e as Error).message);
    } finally {
      setIsUploadingLogo(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  };

  return (
    <div className="rounded-lg border border-slate-200 bg-white p-6">
      <h2 className="text-lg font-semibold text-slate-900">Store Identity</h2>
      <p className="mt-1 text-sm text-slate-600">
        Shown in the app's top bar and on printed receipts. Changes apply immediately, no restart needed.
      </p>

      <div className="mt-4 flex flex-col gap-6 sm:flex-row sm:items-start">
        <div>
          <span className="mb-1.5 block text-xs font-medium text-slate-500">Logo</span>
          <div className="flex items-center gap-3">
            <div className="flex h-16 w-16 shrink-0 items-center justify-center overflow-hidden rounded-lg border border-slate-200 bg-slate-50">
              {isUploadingLogo ? (
                <Loader2 className="h-5 w-5 animate-spin text-slate-400" />
              ) : logoDataUrl ? (
                <img src={logoDataUrl} alt="Store logo" className="h-full w-full object-contain" />
              ) : (
                <ImageOff className="h-6 w-6 text-slate-300" />
              )}
            </div>
            <div>
              <button
                type="button"
                onClick={() => fileInputRef.current?.click()}
                disabled={isUploadingLogo}
                className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-xs font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50"
              >
                <ImagePlus className="h-3.5 w-3.5" />
                Change Logo
              </button>
              <p className="mt-1 text-[11px] text-slate-400">JPG, PNG or SVG, up to 2 MB.</p>
              <input
                ref={fileInputRef}
                type="file"
                accept="image/jpeg,image/png,image/svg+xml"
                className="hidden"
                onChange={(e) => void handleLogoSelected(e.target.files?.[0])}
              />
            </div>
          </div>
          {logoError && <p className="mt-1.5 text-xs text-red-600">{logoError}</p>}
        </div>

        <label className="block flex-1">
          <span className="mb-1.5 block text-xs font-medium text-slate-500">Business name</span>
          <input
            value={nameDraft}
            onChange={(e) => setNameDraft(e.target.value)}
            onBlur={() => void saveNameIfChanged()}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
            disabled={isSavingName}
            className="w-full max-w-xs rounded-md border border-slate-300 px-3 py-2 text-sm focus:border-brand-400 focus:outline-none disabled:opacity-50"
          />
          {isSavingName && <p className="mt-1 text-xs text-slate-400">Saving…</p>}
          {nameError && <p className="mt-1 text-xs text-red-600">{nameError}</p>}
        </label>

        <label className="block flex-1">
          <span className="mb-1.5 block text-xs font-medium text-slate-500">Business phone</span>
          <input
            type="tel"
            value={phoneDraft}
            onChange={(e) => setPhoneDraft(e.target.value)}
            onBlur={() => void savePhoneIfChanged()}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
            disabled={isSavingPhone}
            placeholder="e.g. 0300 1234567"
            className="w-full max-w-xs rounded-md border border-slate-300 px-3 py-2 text-sm focus:border-brand-400 focus:outline-none disabled:opacity-50"
          />
          {isSavingPhone && <p className="mt-1 text-xs text-slate-400">Saving…</p>}
          {phoneError && <p className="mt-1 text-xs text-red-600">{phoneError}</p>}
        </label>

        <label className="block flex-1">
          <span className="mb-1.5 block text-xs font-medium text-slate-500">Delivery number</span>
          <input
            type="tel"
            value={deliveryDraft}
            onChange={(e) => setDeliveryDraft(e.target.value)}
            onBlur={() => void saveDeliveryIfChanged()}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
            disabled={isSavingDelivery}
            placeholder="e.g. 0300 7654321"
            className="w-full max-w-xs rounded-md border border-slate-300 px-3 py-2 text-sm focus:border-brand-400 focus:outline-none disabled:opacity-50"
          />
          <p className="mt-1 text-[11px] text-slate-400">
            Optional — printed on receipts as "Delivery: …" only when set.
          </p>
          {isSavingDelivery && <p className="mt-1 text-xs text-slate-400">Saving…</p>}
          {deliveryError && <p className="mt-1 text-xs text-red-600">{deliveryError}</p>}
        </label>
      </div>
    </div>
  );
}
