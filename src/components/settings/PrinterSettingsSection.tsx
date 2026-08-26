import { useEffect, useState } from "react";
import { Bluetooth, CheckCircle2, Loader2, Printer, Ruler, RefreshCw, Usb } from "lucide-react";
import {
  bluetoothPermissionGranted,
  listBluetoothDevices,
  listWindowsPrinters,
  printDiagnostic,
  requestBluetoothPermission,
} from "../../services/printerService";
import { useAppStore } from "../../store";
import { IS_ANDROID, IS_WINDOWS } from "../../types";
import type { AppConfig, BluetoothDeviceOption, WindowsPrinterOption } from "../../types";

interface PrinterSettingsSectionProps {
  config: AppConfig;
}

/**
 * "Select printer" — Part 1 of the printer-crash fix: printing must never
 * guess at a transport, so this is the one place a cashier/owner actually
 * chooses one. Three platform-specific bodies, matching
 * `printer::escpos::send_to_printer_dispatch` on the Rust side:
 *
 * - **Android**: a Bluetooth picker (paired devices only).
 * - **Windows**: a picker of printers Windows has installed — this exists
 *   because a USB thermal printer with a driver installed can't reliably be
 *   found by raw USB scanning (the driver's own service binds the
 *   interface first — see `printer::windows_spool`'s doc comment), so
 *   Windows needs an explicit by-name selection the same way Android does.
 * - **macOS/Linux**: USB is auto-detected on every print attempt and needs
 *   no selection step, so this renders a short explanatory note instead of
 *   a picker with nothing to pick.
 */
export function PrinterSettingsSection({ config }: PrinterSettingsSectionProps) {
  if (IS_ANDROID) return <BluetoothPrinterSettings config={config} />;
  if (IS_WINDOWS) return <WindowsPrinterSettings config={config} />;
  return <UsbPrinterSettings />;
}

function SectionShell({ description, children }: { description: string; children: React.ReactNode }) {
  return (
    <div className="rounded-lg border border-slate-200 bg-white">
      <div className="border-b border-slate-200 p-6">
        <h2 className="text-lg font-semibold text-slate-900">Printer</h2>
        <p className="mt-1 text-sm text-slate-600">{description}</p>
      </div>
      {children}
      <DiagnosticPrintButton />
    </div>
  );
}

/**
 * "Print width test" — sends a ruler + rows of known length at several
 * candidate widths to whichever printer is currently selected, so a real
 * printer's character-per-line width can be read directly off the paper
 * instead of assumed. Exists because that assumption has already been
 * wrong once (a datasheet's 48 vs. a real printer's measured 42 — see
 * `printer::layout`'s doc comment on the Rust side); if a *different*
 * printer needs a *different* number, this is how to find it without
 * another round of "it printed wrong, send me a photo."
 */
function DiagnosticPrintButton() {
  const [status, setStatus] = useState<"idle" | "printing" | "done" | "error">("idle");
  const [error, setError] = useState<string | null>(null);

  const run = async () => {
    setStatus("printing");
    setError(null);
    try {
      await printDiagnostic();
      setStatus("done");
    } catch (e) {
      setError((e as Error).message);
      setStatus("error");
    }
  };

  return (
    <div className="border-t border-slate-200 px-6 py-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-sm font-medium text-slate-700">Print width test</p>
          <p className="mt-0.5 text-xs text-slate-500">
            Prints a ruler and rows of known length so you can see exactly how many characters this printer fits on
            one line — read off the widest "N=…" line that does <em>not</em> wrap.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void run()}
          disabled={status === "printing"}
          className="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-xs font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50"
        >
          {status === "printing" ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Ruler className="h-3.5 w-3.5" />}
          Print width test
        </button>
      </div>
      {status === "done" && <p className="mt-2 text-xs text-emerald-600">Sent — check the printout.</p>}
      {status === "error" && error && <p className="mt-2 text-xs text-red-600">{error}</p>}
    </div>
  );
}

function UsbPrinterSettings() {
  return (
    <SectionShell description="USB thermal printers are found automatically — plug one in and Print (thermal) on a receipt uses it. No selection needed.">
      <div className="flex items-center gap-2 px-6 py-4 text-sm text-slate-500">
        <Usb className="h-4 w-4 shrink-0" />
        USB, auto-detected on each print
      </div>
    </SectionShell>
  );
}

function BluetoothPrinterSettings({ config }: PrinterSettingsSectionProps) {
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
    void refresh();
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
    <SectionShell description="Choose which paired Bluetooth printer receipts print to.">
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
            <p className="text-sm text-slate-600">Bluetooth permission is needed to see and print to paired printers.</p>
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
    </SectionShell>
  );
}

function WindowsPrinterSettings({ config }: PrinterSettingsSectionProps) {
  const save = useAppStore((state) => state.save);

  const [printers, setPrinters] = useState<WindowsPrinterOption[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState<string | null>(null); // printer name being saved, for a per-row spinner
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    setIsLoading(true);
    setError(null);
    try {
      setPrinters(await listWindowsPrinters());
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
    // Only on mount — this is a manual "Refresh" list, not a live one, same
    // as the rest of Settings.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const selectPrinter = async (printer: WindowsPrinterOption) => {
    setIsSaving(printer.name);
    setError(null);
    try {
      await save({ printerConnectionType: "windows", printerWindowsName: printer.name });
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsSaving(null);
    }
  };

  const selectedName = config.printerConnectionType === "windows" ? config.printerWindowsName : null;

  return (
    <SectionShell description="Choose which installed Windows printer receipts print to — the same one you'd pick in any other application's print dialog.">
      <div className="p-6">
        {error && <p className="mb-3 rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

        {selectedName && (
          <p className="mb-3 flex items-center gap-2 rounded-md bg-emerald-50 px-3 py-2 text-sm text-emerald-700">
            <CheckCircle2 className="h-4 w-4 shrink-0" />
            Selected: {selectedName}
          </p>
        )}

        <div className="mb-3 flex items-center justify-between">
          <p className="text-sm font-medium text-slate-700">Installed printers</p>
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
        ) : printers.length === 0 ? (
          <p className="text-sm text-slate-400">
            No printers found. Install the thermal printer in Windows (Settings &gt; Printers &amp; scanners) first,
            then Refresh.
          </p>
        ) : (
          <ul className="divide-y divide-slate-100 rounded-md border border-slate-100">
            {printers.map((printer) => {
              const isSelected = selectedName === printer.name;
              return (
                <li key={printer.name} className="flex items-center justify-between px-3 py-2.5">
                  <span className="flex min-w-0 items-center gap-2">
                    <Printer className="h-4 w-4 shrink-0 text-slate-400" />
                    <span className="truncate text-sm font-medium text-slate-900">{printer.name}</span>
                  </span>
                  <button
                    type="button"
                    onClick={() => void selectPrinter(printer)}
                    disabled={isSaving === printer.name || isSelected}
                    className={[
                      "shrink-0 rounded-md px-3 py-1.5 text-xs font-semibold",
                      isSelected
                        ? "bg-emerald-50 text-emerald-700"
                        : "bg-brand-600 text-white hover:bg-brand-700 disabled:opacity-50",
                    ].join(" ")}
                  >
                    {isSaving === printer.name ? "Saving…" : isSelected ? "Selected" : "Select"}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </SectionShell>
  );
}
