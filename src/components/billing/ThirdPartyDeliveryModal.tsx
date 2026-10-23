import { useEffect, useState, type FormEvent } from "react";
import { Store, UserCheck } from "lucide-react";
import { getRecentRiders, listActivePlatforms } from "../../services/deliveryService";
import { reportWarning } from "../../services/logService";
import { useBillingStore } from "../../store";
import type { DeliveryDetails, DeliveryPlatform, RecentRider } from "../../types";
import { ModalHeader } from "../ui/ModalCard";
import { ModalOverlay } from "../ui/ModalOverlay";

interface ThirdPartyDeliveryModalProps {
  onClose: () => void;
  onSaved: (details: DeliveryDetails) => void;
}

export function ThirdPartyDeliveryModal({ onClose, onSaved }: ThirdPartyDeliveryModalProps) {
  const currentDetails = useBillingStore((state) => state.deliveryDetails);
  const customerName = useBillingStore((state) => state.customerName);
  const setCustomerName = useBillingStore((state) => state.setCustomerName);

  const [platforms, setPlatforms] = useState<DeliveryPlatform[]>([]);
  const [selectedPlatformId, setSelectedPlatformId] = useState<number | null>(
    currentDetails?.deliveryPlatformId ?? null
  );
  const [platformOrderRef, setPlatformOrderRef] = useState(
    currentDetails?.platformOrderRef || ""
  );

  const [riderName, setRiderName] = useState(currentDetails?.deliveryRiderName || "");
  const [riderPhone, setRiderPhone] = useState(currentDetails?.deliveryRiderPhone || "");
  const [recentRiders, setRecentRiders] = useState<RecentRider[]>([]);

  const [custName, setCustName] = useState(customerName || "");
  const [custPhone, setCustPhone] = useState(currentDetails?.customerPhone || "");

  const [isLoadingPlatforms, setIsLoadingPlatforms] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Load platforms on mount
  useEffect(() => {
    let cancelled = false;
    listActivePlatforms()
      .then((data) => {
        if (cancelled) return;
        setPlatforms(data);
        if (data.length > 0 && selectedPlatformId === null) {
          setSelectedPlatformId(data[0].id);
        }
      })
      .catch((err) => {
        reportWarning(`Failed to load active delivery platforms: ${(err as Error).message}`);
      })
      .finally(() => {
        if (!cancelled) setIsLoadingPlatforms(false);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  // When selected platform changes, fetch recent riders from SQLite DB
  useEffect(() => {
    if (!selectedPlatformId) {
      setRecentRiders([]);
      return;
    }
    let cancelled = false;
    getRecentRiders(selectedPlatformId, 5)
      .then((riders) => {
        if (!cancelled) setRecentRiders(riders);
      })
      .catch((err) => {
        reportWarning(`Failed to fetch recent platform riders: ${(err as Error).message}`);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedPlatformId]);

  const handleSelectRecentRider = (rider: RecentRider) => {
    setRiderName(rider.name);
    setRiderPhone(rider.phone);
  };

  const handleSave = (e: FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!selectedPlatformId) {
      setError("Please select a delivery platform");
      return;
    }

    const platform = platforms.find((p) => p.id === selectedPlatformId);
    if (!platform) {
      setError("Platform not found");
      return;
    }

    const details: DeliveryDetails = {
      riderSource: "manual",
      deliveryRiderId: null,
      deliveryRiderName: riderName.trim() || null,
      deliveryRiderPhone: riderPhone.trim() || null,
      deliveryPlatformId: platform.id,
      deliveryPlatformName: platform.name,
      customerPhone: custPhone.trim() || null,
      deliveryAddress: null,
      deliveryChargeMinor: 0,
      deliveryStatus: "placed",
      platformOrderRef: platformOrderRef.trim() || null,
    };

    if (custName.trim()) {
      setCustomerName(custName.trim());
    }

    onSaved(details);
  };

  return (
    <ModalOverlay>
      <div className="w-full max-w-lg rounded-2xl border border-line bg-surface p-6 shadow-soft-lg">
        <ModalHeader
          icon={Store}
          title="Third-Party Platform Delivery"
          subtitle="Foodpanda, Cheetay, or other external aggregator order"
          onClose={onClose}
          className="pb-4"
        />

        <form onSubmit={handleSave} className="mt-5 space-y-4">
          {error && (
            <div className="rounded-xl bg-danger-soft p-3 text-xs text-danger-text">
              {error}
            </div>
          )}

          {/* Platform and Order Reference */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <div>
              <label className="mb-1 block text-xs font-medium text-fg">
                Platform <span className="text-danger-text">*</span>
              </label>
              {isLoadingPlatforms ? (
                <div className="h-9 rounded-xl bg-surface-2 animate-pulse" />
              ) : platforms.length === 0 ? (
                <div className="rounded-xl bg-surface-2 p-2 text-xs text-fg-muted">
                  No active platforms found. Add one in Settings &gt; Delivery Platforms.
                </div>
              ) : (
                <select
                  value={selectedPlatformId ?? ""}
                  onChange={(e) => setSelectedPlatformId(Number(e.target.value))}
                  className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent"
                >
                  {platforms.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              )}
            </div>

            <div>
              <label className="mb-1 block text-xs font-medium text-fg">
                Platform Order Ref # (Optional)
              </label>
              <input
                type="text"
                placeholder="e.g. #FP-9821"
                value={platformOrderRef}
                onChange={(e) => setPlatformOrderRef(e.target.value)}
                className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent"
              />
            </div>
          </div>

          {/* Platform Rider Details & Autosuggest */}
          <div className="rounded-xl border border-line p-3.5 space-y-3">
            <div className="flex items-center justify-between">
              <label className="text-xs font-semibold text-fg">Platform Rider Info</label>
              {recentRiders.length > 0 && (
                <span className="text-[11px] text-fg-muted">Recent riders autosuggest</span>
              )}
            </div>

            {/* Quick Chips for Recent Riders from SQLite */}
            {recentRiders.length > 0 && (
              <div className="flex flex-wrap gap-1.5 pt-0.5">
                {recentRiders.map((r, i) => (
                  <button
                    key={`${r.phone}-${i}`}
                    type="button"
                    onClick={() => handleSelectRecentRider(r)}
                    className="inline-flex items-center gap-1 rounded-lg border border-line bg-surface-2 px-2 py-1 text-xs text-fg hover:border-accent hover:bg-accent-soft transition"
                    title={`Click to fill ${r.name} (${r.phone})`}
                  >
                    <UserCheck className="h-3 w-3 text-accent-text" />
                    <span>{r.name}</span>
                    <span className="text-[10px] text-fg-muted">({r.phone})</span>
                  </button>
                ))}
              </div>
            )}

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-1">
              <div>
                <label className="mb-1 block text-xs font-medium text-fg-muted">
                  Rider Name
                </label>
                <input
                  type="text"
                  placeholder="e.g. Kamran"
                  value={riderName}
                  onChange={(e) => setRiderName(e.target.value)}
                  className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent"
                />
              </div>
              <div>
                <label className="mb-1 block text-xs font-medium text-fg-muted">
                  Rider Phone
                </label>
                <input
                  type="tel"
                  placeholder="e.g. 0300-7654321"
                  value={riderPhone}
                  onChange={(e) => setRiderPhone(e.target.value)}
                  className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent"
                />
              </div>
            </div>
          </div>

          {/* Customer info (Optional for 3rd party delivery) */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <div>
              <label className="mb-1 block text-xs font-medium text-fg">
                Customer Name (Optional)
              </label>
              <input
                type="text"
                placeholder="e.g. Sara"
                value={custName}
                onChange={(e) => setCustName(e.target.value)}
                className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent"
              />
            </div>
            <div>
              <label className="mb-1 block text-xs font-medium text-fg">
                Customer Phone (Optional)
              </label>
              <input
                type="tel"
                placeholder="e.g. 0321-9876543"
                value={custPhone}
                onChange={(e) => setCustPhone(e.target.value)}
                className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent"
              />
            </div>
          </div>

          <div className="rounded-xl bg-surface-2 p-3 text-xs text-fg-muted flex items-center justify-between">
            <span>Payment Method:</span>
            <span className="font-semibold text-accent-text uppercase">Platform (Non-Cash Drawer)</span>
          </div>

          <div className="flex items-center justify-end gap-2.5 pt-2 border-t border-line">
            <button
              type="button"
              onClick={onClose}
              className="rounded-xl border border-line px-4 py-2 text-sm font-medium text-fg-muted hover:bg-surface-2 transition"
            >
              Cancel
            </button>
            <button
              type="submit"
              className="rounded-xl bg-accent px-5 py-2 text-sm font-semibold text-accent-contrast shadow-soft hover:opacity-90 transition"
            >
              Apply Platform Info
            </button>
          </div>
        </form>
      </div>
    </ModalOverlay>
  );
}
