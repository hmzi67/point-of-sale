import { useEffect, useState, type FormEvent } from "react";
import { Bike, X } from "lucide-react";
import { getUsers } from "../../services/authService";
import { devWarn } from "../../services/logService";
import { useBillingStore } from "../../store";
import type { DeliveryDetails, User } from "../../types";
import { decimalToMinor, minorToDecimal } from "../../utils/format";
import { ModalOverlay } from "../ui/ModalOverlay";

interface DeliveryDetailsModalProps {
  onClose: () => void;
  onSaved: (details: DeliveryDetails) => void;
}

export function DeliveryDetailsModal({ onClose, onSaved }: DeliveryDetailsModalProps) {
  const currentDetails = useBillingStore((state) => state.deliveryDetails);
  const customerName = useBillingStore((state) => state.customerName);
  const setCustomerName = useBillingStore((state) => state.setCustomerName);

  const [name, setName] = useState(customerName || "");
  const [phone, setPhone] = useState(currentDetails?.customerPhone || "");
  const [address, setAddress] = useState(currentDetails?.deliveryAddress || "");
  const [chargeDecimal, setChargeDecimal] = useState<number>(
    currentDetails?.deliveryChargeMinor ? minorToDecimal(currentDetails.deliveryChargeMinor) : 0
  );

  const [riderSource, setRiderSource] = useState<"user" | "manual">(
    currentDetails?.riderSource === "manual" ? "manual" : "user"
  );
  const [selectedRiderId, setSelectedRiderId] = useState<number | null>(
    currentDetails?.deliveryRiderId ?? null
  );
  const [manualRiderName, setManualRiderName] = useState(currentDetails?.deliveryRiderName || "");
  const [manualRiderPhone, setManualRiderPhone] = useState(currentDetails?.deliveryRiderPhone || "");

  const [deliveryRiders, setDeliveryRiders] = useState<User[]>([]);
  const [isLoadingRiders, setIsLoadingRiders] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getUsers()
      .then((users) => {
        if (cancelled) return;
        const riders = users.filter((u) => u.role === "delivery");
        setDeliveryRiders(riders);
        if (riders.length === 0 && riderSource === "user") {
          setRiderSource("manual");
        } else if (riders.length > 0 && selectedRiderId === null) {
          setSelectedRiderId(riders[0].id);
        }
      })
      .catch((err) => {
        devWarn("Failed to load riders", err);
      })
      .finally(() => {
        if (!cancelled) setIsLoadingRiders(false);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const handleSave = (e: FormEvent) => {
    e.preventDefault();
    setError(null);

    const cleanPhone = phone.trim();
    if (!cleanPhone) {
      setError("Customer phone number is required");
      return;
    }

    const cleanAddress = address.trim();
    if (!cleanAddress) {
      setError("Delivery address is required");
      return;
    }

    let riderName: string | null = null;
    let riderPhone: string | null = null;
    let riderId: number | null = null;

    if (riderSource === "user") {
      if (!selectedRiderId) {
        setError("Please select an in-house delivery rider, or switch to manual entry");
        return;
      }
      const rider = deliveryRiders.find((r) => r.id === selectedRiderId);
      if (!rider) {
        setError("Selected rider was not found");
        return;
      }
      riderId = rider.id;
      riderName = rider.name;
    } else {
      if (!manualRiderName.trim()) {
        setError("Manual rider name is required");
        return;
      }
      if (!manualRiderPhone.trim()) {
        setError("Manual rider phone is required");
        return;
      }
      riderName = manualRiderName.trim();
      riderPhone = manualRiderPhone.trim();
    }

    const chargeMinor = decimalToMinor(Math.max(0, chargeDecimal || 0));

    const details: DeliveryDetails = {
      riderSource,
      deliveryRiderId: riderId,
      deliveryRiderName: riderName,
      deliveryRiderPhone: riderPhone,
      deliveryPlatformId: null,
      deliveryPlatformName: null,
      customerPhone: cleanPhone,
      deliveryAddress: cleanAddress,
      deliveryChargeMinor: chargeMinor,
      deliveryStatus: "placed",
      platformOrderRef: null,
    };

    if (name.trim()) {
      setCustomerName(name.trim());
    }

    onSaved(details);
  };

  return (
    <ModalOverlay>
      <div className="w-full max-w-lg rounded-2xl border border-line bg-surface p-6 shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-line pb-4">
          <div className="flex items-center gap-2.5">
            <div className="rounded-xl bg-accent-soft p-2 text-accent-text">
              <Bike className="h-5 w-5" />
            </div>
            <div>
              <h2 className="text-lg font-semibold text-fg">In-House Delivery Details</h2>
              <p className="text-xs text-fg-muted">
                Assign rider and customer delivery information
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg p-1.5 text-fg-muted hover:bg-surface-2"
            aria-label="Close"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        <form onSubmit={handleSave} className="mt-5 space-y-4">
          {error && (
            <div className="rounded-xl bg-danger-soft p-3 text-xs text-danger-text">
              {error}
            </div>
          )}

          {/* Customer Details */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <div>
              <label className="mb-1 block text-xs font-medium text-fg">
                Customer Name (Optional)
              </label>
              <input
                type="text"
                placeholder="e.g. Ali Ahmed"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent"
              />
            </div>
            <div>
              <label className="mb-1 block text-xs font-medium text-fg">
                Customer Phone <span className="text-danger-text">*</span>
              </label>
              <input
                type="tel"
                required
                placeholder="e.g. 0300-1234567"
                value={phone}
                onChange={(e) => setPhone(e.target.value)}
                className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent"
              />
            </div>
          </div>

          <div>
            <label className="mb-1 block text-xs font-medium text-fg">
              Delivery Address <span className="text-danger-text">*</span>
            </label>
            <textarea
              required
              rows={2}
              placeholder="e.g. House 14, Street 3, Sector F-7/2..."
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent resize-none"
            />
          </div>

          {/* Rider Selection Source */}
          <div className="rounded-xl border border-line p-3.5 space-y-3">
            <div className="flex items-center justify-between">
              <label className="text-xs font-semibold text-fg">Rider Assignment</label>
              <div className="flex rounded-lg bg-surface-2 p-0.5 border border-line">
                <button
                  type="button"
                  onClick={() => setRiderSource("user")}
                  disabled={deliveryRiders.length === 0}
                  className={`rounded-md px-2.5 py-1 text-xs font-medium transition ${
                    riderSource === "user"
                      ? "bg-surface shadow text-fg font-semibold"
                      : "text-fg-muted hover:text-fg disabled:opacity-40"
                  }`}
                >
                  In-House Staff
                </button>
                <button
                  type="button"
                  onClick={() => setRiderSource("manual")}
                  className={`rounded-md px-2.5 py-1 text-xs font-medium transition ${
                    riderSource === "manual"
                      ? "bg-surface shadow text-fg font-semibold"
                      : "text-fg-muted hover:text-fg"
                  }`}
                >
                  Manual Entry
                </button>
              </div>
            </div>

            {riderSource === "user" ? (
              <div>
                {isLoadingRiders ? (
                  <p className="text-xs text-fg-muted py-2">Loading riders...</p>
                ) : deliveryRiders.length === 0 ? (
                  <div className="rounded-lg bg-surface-2 p-2.5 text-xs text-fg-muted">
                    No users with role <span className="font-semibold text-fg">delivery</span> found. Create one in Settings &gt; Manage Users or use Manual Entry.
                  </div>
                ) : (
                  <div>
                    <label className="mb-1 block text-xs font-medium text-fg-muted">
                      Select Rider
                    </label>
                    <select
                      value={selectedRiderId ?? ""}
                      onChange={(e) => setSelectedRiderId(Number(e.target.value))}
                      className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent"
                    >
                      {deliveryRiders.map((r) => (
                        <option key={r.id} value={r.id}>
                          {r.name}
                        </option>
                      ))}
                    </select>
                  </div>
                )}
              </div>
            ) : (
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-1">
                <div>
                  <label className="mb-1 block text-xs font-medium text-fg">
                    Rider Name <span className="text-danger-text">*</span>
                  </label>
                  <input
                    type="text"
                    required
                    placeholder="e.g. Tariq"
                    value={manualRiderName}
                    onChange={(e) => setManualRiderName(e.target.value)}
                    className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent"
                  />
                </div>
                <div>
                  <label className="mb-1 block text-xs font-medium text-fg">
                    Rider Phone <span className="text-danger-text">*</span>
                  </label>
                  <input
                    type="tel"
                    required
                    placeholder="e.g. 0312-3456789"
                    value={manualRiderPhone}
                    onChange={(e) => setManualRiderPhone(e.target.value)}
                    className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent"
                  />
                </div>
              </div>
            )}
          </div>

          {/* Delivery Charge */}
          <div>
            <label className="mb-1 block text-xs font-medium text-fg">
              Delivery Charge (PKR)
            </label>
            <input
              type="number"
              min="0"
              step="10"
              placeholder="0"
              value={chargeDecimal === 0 ? "" : chargeDecimal}
              onChange={(e) => setChargeDecimal(parseFloat(e.target.value) || 0)}
              className="w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg outline-none transition focus:border-accent"
            />
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
              Apply Delivery Info
            </button>
          </div>
        </form>
      </div>
    </ModalOverlay>
  );
}
