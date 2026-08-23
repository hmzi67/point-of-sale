import { useEffect, useState } from "react";
import { Bluetooth, CheckCircle2, Loader2, RefreshCw, Usb } from "lucide-react";
import { bluetoothPermissionGranted, listBluetoothDevices, requestBluetoothPermission } from "../../services/printerService";
import { useAppStore } from "../../store";
import { IS_ANDROID } from "../../types";
import type { AppConfig, BluetoothDeviceOption } from "../../types";

interface PrinterSettingsSectionProps {
  config: AppConfig;
}

/**
 * "Select printer" — Part 1 of the printer-crash fix: printing must never
 * guess at a transport, so this is the one place a cashier/owner actually
 * chooses one. Android-only UI (Bluetooth picker); desktop's USB transport
 * is auto-detected on every print attempt and needs no selection step (see
 * `printer::escpos`'s module doc comment on the Rust side), so this renders
 * a short explanatory note there instead of a picker with nothing to pick.
 */
export function PrinterSettingsSection({ config }: PrinterSettingsSectionProps) {
  const save = useAppStore((state) => state.save);

  const [permissionGranted, setPermissionGranted] = useState<boolean | null>(null);
  const [devices, setDevices] = useState<BluetoothDeviceOption[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState<string | null>(null); // address being saved, for a per-row spinner
  const [error, setError] = useState<string | null>(null);
  const [requestingPermission, setRequestingPermission] = useState(false);

  const refresh = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const granted = await bluetoothPermissionGranted();
      setPermissionGranted(granted);
      if (granted) {
        setDevices(await listBluetoothDevices());
      }
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    if (IS_ANDROID) void refresh();
    // Only on mount — this is a manual "Refresh" list, not a live one, same
    // as the rest of Settings.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const grantPermission = async () => {
    setRequestingPermission(true);
    setError(null);
    try {
      // Fire-and-forget — see `printerService.requestBluetoothPermission`'s
      // doc comment. The OS dialog is synchronous from the user's point of
      // view even though this call isn't, so re-checking right after it
      // resolves (once the user has answered) reflects their choice.
      await requestBluetoothPermission();
      await refresh();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setRequestingPermission(false);
    }
  };

  const selectDevice = async (device: BluetoothDeviceOption) => {
    setIsSaving(device.address);
    setError(null);
    try {
      await save({
        printerConnectionType: "bluetooth",
        printerBluetoothAddress: device.address,
        printerBluetoothName: device.name,
      });
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsSaving(null);
    }
  };

  const selected =
    config.printerConnectionType === "bluetooth" && config.printerBluetoothAddress
      ? { name: config.printerBluetoothName ?? "(unnamed device)", address: config.printerBluetoothAddress }
      : null;

  return (
    <div className="rounded-lg border border-slate-200 bg-white">
      <div className="border-b border-slate-200 p-6">
        <h2 className="text-lg font-semibold text-slate-900">Printer</h2>
        <p className="mt-1 text-sm text-slate-600">
          {IS_ANDROID
            ? "Choose which paired Bluetooth printer receipts print to."
            : "USB thermal printers are found automatically — plug one in and Print (thermal) on a receipt uses it. No selection needed."}
        </p>
      </div>

      {!IS_ANDROID ? (
        <div className="flex items-center gap-2 px-6 py-4 text-sm text-slate-500">
          <Usb className="h-4 w-4 shrink-0" />
          USB, auto-detected on each print
        </div>
      ) : (
        <div className="p-6">
          {error && <p className="mb-3 rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

          {selected && (
            <p className="mb-3 flex items-center gap-2 rounded-md bg-emerald-50 px-3 py-2 text-sm text-emerald-700">
              <CheckCircle2 className="h-4 w-4 shrink-0" />
              Selected: {selected.name}
            </p>
          )}

          {permissionGranted === false ? (
            <div className="space-y-2">
              <p className="text-sm text-slate-600">
                Bluetooth permission is needed to see and print to paired printers.
              </p>
              <button
                type="button"
                onClick={() => void grantPermission()}
                disabled={requestingPermission}
                className="inline-flex items-center gap-1.5 rounded-md bg-brand-600 px-3 py-2 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-50"
              >
                {requestingPermission ? <Loader2 className="h-4 w-4 animate-spin" /> : <Bluetooth className="h-4 w-4" />}
                Grant Bluetooth permission
              </button>
            </div>
          ) : (
            <>
              <div className="mb-3 flex items-center justify-between">
                <p className="text-sm font-medium text-slate-700">Paired devices</p>
                <button
                  type="button"
                  onClick={() => void refresh()}
                  disabled={isLoading}
                  className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-slate-500 hover:bg-slate-100 disabled:opacity-50"
                >
                  <RefreshCw className={`h-3.5 w-3.5 ${isLoading ? "animate-spin" : ""}`} />
                  Refresh
                </button>
              </div>

              {isLoading ? (
                <p className="text-sm text-slate-400">Loading…</p>
              ) : devices.length === 0 ? (
                <p className="text-sm text-slate-400">
                  No paired devices found. Pair the printer in your phone's Bluetooth settings first, then Refresh.
                </p>
              ) : (
                <ul className="divide-y divide-slate-100 rounded-md border border-slate-100">
                  {devices.map((device) => {
                    const isSelected = selected?.address === device.address;
                    return (
                      <li key={device.address} className="flex items-center justify-between px-3 py-2.5">
                        <span className="min-w-0">
                          <span className="block truncate text-sm font-medium text-slate-900">{device.name}</span>
                          <span className="block text-xs text-slate-400">{device.address}</span>
                        </span>
                        <button
                          type="button"
                          onClick={() => void selectDevice(device)}
                          disabled={isSaving === device.address || isSelected}
                          className={[
                            "shrink-0 rounded-md px-3 py-1.5 text-xs font-semibold",
                            isSelected
                              ? "bg-emerald-50 text-emerald-700"
                              : "bg-brand-600 text-white hover:bg-brand-700 disabled:opacity-50",
                          ].join(" ")}
                        >
                          {isSaving === device.address ? "Saving…" : isSelected ? "Selected" : "Select"}
                        </button>
                      </li>
                    );
                  })}
                </ul>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
