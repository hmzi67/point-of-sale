import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Bluetooth,
  CheckCircle2,
  Loader2,
  Printer,
  Ruler,
  RefreshCw,
  Usb,
} from "lucide-react";
import {
  bluetoothPermissionGranted,
  listBluetoothDevices,
  listUsbDevices,
  listWindowsPrinters,
  printDiagnostic,
  requestBluetoothPermission,
  requestUsbPermission,
  usbPermissionGranted,
} from "../../services/printerService";
import { useAppStore } from "../../store";
import { IS_ANDROID, IS_WINDOWS } from "../../types";
import type { AppConfig, BluetoothDeviceOption, UsbDeviceOption, WindowsPrinterOption } from "../../types";

interface PrinterSettingsSectionProps {
  config: AppConfig;
}

/**
 * "Select printer" — printing must never guess at a transport, so this is the
 * one place a cashier/owner actually chooses one. Three platform-specific
 * bodies, matching `printer::escpos::send_to_printer_dispatch` on the Rust
 * side:
 *
 * - **Android**: two transports the client picks between —
 *   Bluetooth (paired devices) or USB (an OTG-attached printer). USB exists
 *   because plenty of till printers have no Bluetooth radio at all; the
 *   client hardware that forced it, a Gainscha GA-E200, is labelled
 *   "Interface: Serial & USB & Ethernet".
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
  if (IS_ANDROID) return <AndroidPrinterSettings config={config} />;
  if (IS_WINDOWS) return <WindowsPrinterSettings config={config} />;
  return <DesktopUsbPrinterSettings />;
}

function SectionShell({ description, children }: { description: string; children: React.ReactNode }) {
  return (
    <div className="rounded-lg border border-line bg-surface">
      <div className="border-b border-line p-6">
        <h2 className="text-lg font-semibold text-fg">Printer</h2>
        <p className="mt-1 text-sm text-fg-2">{description}</p>
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
 *
 * Transport-agnostic on purpose: it goes through the same
 * `send_to_printer` dispatch as a real receipt, so a USB-connected printer
 * is measured exactly the way a Bluetooth one is. Paper width is a property
 * of the printer, not of the cable.
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
    <div className="border-t border-line px-6 py-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-sm font-medium text-fg-2">Print width test</p>
          <p className="mt-0.5 text-xs text-fg-muted">
            Prints a ruler and rows of known length so you can see exactly how many characters this printer fits on
            one line — read off the widest "N=…" line that does <em>not</em> wrap.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void run()}
          disabled={status === "printing"}
          className="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-line-strong px-3 py-1.5 text-xs font-medium text-fg-2 hover:bg-surface-2 disabled:opacity-50"
        >
          {status === "printing" ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Ruler className="h-3.5 w-3.5" />}
          Print width test
        </button>
      </div>
      {status === "done" && <p className="mt-2 text-xs text-success-text">Sent — check the printout.</p>}
      {status === "error" && error && <p className="mt-2 text-xs text-danger-text">{error}</p>}
    </div>
  );
}

function DesktopUsbPrinterSettings() {
  return (
    <SectionShell description="USB thermal printers are found automatically — plug one in and Print (thermal) on a receipt uses it. No selection needed.">
      <div className="flex items-center gap-2 px-6 py-4 text-sm text-fg-muted">
        <Usb className="h-4 w-4 shrink-0" />
        USB, auto-detected on each print
      </div>
    </SectionShell>
  );
}

function ErrorNote({ message }: { message: string }) {
  return <p className="mb-3 rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{message}</p>;
}

function SelectedNote({ label }: { label: string }) {
  return (
    <p className="mb-3 flex items-center gap-2 rounded-md bg-success-soft px-3 py-2 text-sm text-success-text">
      <CheckCircle2 className="h-4 w-4 shrink-0" />
      Selected: {label}
    </p>
  );
}

function ListHeader({ label, isLoading, onRefresh }: { label: string; isLoading: boolean; onRefresh: () => void }) {
  return (
    <div className="mb-3 flex items-center justify-between">
      <p className="text-sm font-medium text-fg-2">{label}</p>
      <button
        type="button"
        onClick={onRefresh}
        disabled={isLoading}
        className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-fg-muted hover:bg-surface-3 disabled:opacity-50"
      >
        <RefreshCw className={`h-3.5 w-3.5 ${isLoading ? "animate-spin" : ""}`} />
        Refresh
      </button>
    </div>
  );
}

type AndroidConnectionType = "bluetooth" | "usb";

const ANDROID_DESCRIPTIONS: Record<AndroidConnectionType, string> = {
  bluetooth: "Choose which paired Bluetooth printer receipts print to.",
  usb: "Choose which USB printer, connected through an OTG cable, receipts print to.",
};

/**
 * The Android printer section: a connection-type switch over two independent
 * pickers.
 *
 * The switch is not cosmetic — it decides which native APIs get touched at
 * all. Only the selected tab's body mounts, so a shop configured for USB
 * never calls a Bluetooth command, and vice versa. That is what stopped the
 * Bluetooth adapter check (and the "isEnabled check failed" error it used to
 * raise) from appearing on installs that print over USB and have no business
 * asking about a Bluetooth radio.
 *
 * The tab starts on whatever `printerConnectionType` is stored, defaulting to
 * Bluetooth — so an install that was already configured for Bluetooth opens
 * exactly where it did before this second transport existed. Switching tabs
 * changes nothing that is saved; picking a device is what commits both the
 * device and the connection type.
 */
function AndroidPrinterSettings({ config }: PrinterSettingsSectionProps) {
  const [mode, setMode] = useState<AndroidConnectionType>(
    config.printerConnectionType === "usb" ? "usb" : "bluetooth",
  );

  // Only meaningful once *something* has been selected: before that,
  // `printerConnectionType` is null and there is nothing to contradict.
  const savedMode = config.printerConnectionType === "usb" || config.printerConnectionType === "bluetooth"
    ? config.printerConnectionType
    : null;
  const isBrowsingOtherMode = savedMode !== null && savedMode !== mode;

  return (
    <SectionShell description={ANDROID_DESCRIPTIONS[mode]}>
      <div className="px-6 pt-6">
        <div className="inline-flex rounded-md border border-line bg-surface-2 p-0.5" role="tablist">
          {(["bluetooth", "usb"] as const).map((option) => {
            const isActive = mode === option;
            const Icon = option === "bluetooth" ? Bluetooth : Usb;
            return (
              <button
                key={option}
                type="button"
                role="tab"
                aria-selected={isActive}
                onClick={() => setMode(option)}
                className={[
                  "inline-flex items-center gap-1.5 rounded px-3 py-1.5 text-sm font-medium transition-colors",
                  isActive ? "bg-surface text-fg shadow-sm" : "text-fg-muted hover:text-fg-2",
                ].join(" ")}
              >
                <Icon className="h-4 w-4" />
                {option === "bluetooth" ? "Bluetooth" : "USB"}
              </button>
            );
          })}
        </div>

        {isBrowsingOtherMode && (
          <p className="mt-3 flex items-start gap-2 rounded-md bg-warning-soft px-3 py-2 text-xs text-warning-text">
            <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
            Receipts still print over {savedMode === "usb" ? "USB" : "Bluetooth"}. Pick a printer below to switch to{" "}
            {mode === "usb" ? "USB" : "Bluetooth"}.
          </p>
        )}
      </div>

      {mode === "bluetooth" ? <BluetoothPrinterPicker config={config} /> : <AndroidUsbPrinterPicker config={config} />}
    </SectionShell>
  );
}

function BluetoothPrinterPicker({ config }: PrinterSettingsSectionProps) {
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
    // as the rest of Settings. Mount only happens on the Bluetooth tab, which
    // is what keeps these calls off USB-configured installs entirely.
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
    <div className="p-6">
      {error && <ErrorNote message={error} />}
      {selected && <SelectedNote label={selected.name} />}

      {permissionGranted === false ? (
        <div className="space-y-2">
          <p className="text-sm text-fg-2">Bluetooth permission is needed to see and print to paired printers.</p>
          <button
            type="button"
            onClick={() => void grantPermission()}
            disabled={requestingPermission}
            className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-2 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:opacity-50"
          >
            {requestingPermission ? <Loader2 className="h-4 w-4 animate-spin" /> : <Bluetooth className="h-4 w-4" />}
            Grant Bluetooth permission
          </button>
          <p className="text-xs text-fg-muted">
            If the printer connects by cable instead, switch to USB above — no Bluetooth permission is needed there.
          </p>
        </div>
      ) : (
        <>
          <ListHeader label="Paired devices" isLoading={isLoading} onRefresh={() => void refresh()} />

          {isLoading ? (
            <p className="text-sm text-fg-muted">Loading…</p>
          ) : devices.length === 0 ? (
            <p className="text-sm text-fg-muted">
              No paired devices found. Pair the printer in your phone's Bluetooth settings first, then Refresh.
            </p>
          ) : (
            <ul className="divide-y divide-line rounded-md border border-line">
              {devices.map((device) => {
                const isSelected = selected?.address === device.address;
                return (
                  <li key={device.address} className="flex items-center justify-between px-3 py-2.5">
                    <span className="min-w-0">
                      <span className="block truncate text-sm font-medium text-fg">{device.name}</span>
                      <span className="block text-xs text-fg-muted">{device.address}</span>
                    </span>
                    <button
                      type="button"
                      onClick={() => void selectDevice(device)}
                      disabled={isSaving === device.address || isSelected}
                      className={[
                        "shrink-0 rounded-md px-3 py-1.5 text-xs font-semibold",
                        isSelected
                          ? "bg-success-soft text-success-text"
                          : "bg-accent text-accent-ink hover:bg-accent-hover disabled:opacity-50",
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
  );
}

/** How long to keep re-checking a USB permission after the system dialog has
 * been shown, and how often. Android's `requestPermission` returns as soon as
 * the dialog is *up*, not when it's answered, and nothing in this app
 * receives the grant broadcast (see `android_usb::request_permission` for why
 * that's deliberate) — so the answer has to be observed by asking again. The
 * window is generous because the user has to read a dialog, and the poll is
 * cheap: one `hasPermission` call against an already-enumerated device. */
const USB_PERMISSION_POLL_MS = 700;
const USB_PERMISSION_TIMEOUT_MS = 60_000;

/**
 * The USB (OTG) printer picker.
 *
 * Two things here differ from the Bluetooth picker in ways that come from
 * Android rather than from taste:
 *
 * 1. **Permission is per device, not per app.** There is no single "USB
 *    permission" to grant once — the user approves one `UsbDevice`, and the
 *    approval is dropped when that device is unplugged. So permission state
 *    lives on each row, and a printer that worked yesterday can need
 *    re-approving today. Nothing is broken when that happens; the row just
 *    offers "Allow access" again.
 * 2. **Nothing is filtered by device class.** The standard Printer class
 *    (0x07) is shown as a badge and sorts matching devices to the top, but
 *    the client's Gainscha GA-E200 — the printer this mode was written for —
 *    reports a vendor-specific class instead, and so do many thermal
 *    printers. A class filter would have hidden exactly the hardware that
 *    needed listing, so the user picks from everything attached.
 */
function AndroidUsbPrinterPicker({ config }: PrinterSettingsSectionProps) {
  const save = useAppStore((state) => state.save);

  const [devices, setDevices] = useState<UsbDeviceOption[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState<string | null>(null); // device id being saved, for a per-row spinner
  const [awaitingPermission, setAwaitingPermission] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Guards the permission poll below against setting state after the user has
  // navigated away mid-dialog.
  const isMounted = useRef(true);
  useEffect(() => {
    isMounted.current = true;
    return () => {
      isMounted.current = false;
    };
  }, []);

  const refresh = async () => {
    setIsLoading(true);
    setError(null);
    try {
      setDevices(await listUsbDevices());
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
    // Only on mount — a manual "Refresh" list, same as every other picker in
    // Settings. Replugging the printer is a Refresh away.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** Shows the system dialog for `deviceId`, then watches for the answer —
   * see `USB_PERMISSION_POLL_MS` for why watching is necessary. */
  const grantAccess = async (deviceId: string) => {
    setAwaitingPermission(deviceId);
    setError(null);
    try {
      await requestUsbPermission(deviceId);

      const deadline = Date.now() + USB_PERMISSION_TIMEOUT_MS;
      while (Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, USB_PERMISSION_POLL_MS));
        if (!isMounted.current) return;
        if (await usbPermissionGranted(deviceId)) break;
      }

      if (isMounted.current) await refresh();
    } catch (e) {
      if (isMounted.current) setError((e as Error).message);
    } finally {
      if (isMounted.current) setAwaitingPermission(null);
    }
  };

  const selectDevice = async (device: UsbDeviceOption) => {
    setIsSaving(device.id);
    setError(null);
    try {
      await save({
        printerConnectionType: "usb",
        printerUsbDevice: device.id,
        printerUsbName: device.name,
      });
    } catch (e) {
      setError((e as Error).message);
      return;
    } finally {
      setIsSaving(null);
    }

    // Saving the choice and holding the OS grant are separate steps, so the
    // dialog follows the selection rather than needing a second deliberate
    // tap for the case that always applies on a fresh pick.
    if (!device.hasPermission) await grantAccess(device.id);
  };

  const selectedId = config.printerConnectionType === "usb" ? config.printerUsbDevice : null;

  return (
    <div className="p-6">
      {error && <ErrorNote message={error} />}
      {selectedId && <SelectedNote label={config.printerUsbName ?? selectedId} />}

      <ListHeader label="Connected USB devices" isLoading={isLoading} onRefresh={() => void refresh()} />

      {isLoading ? (
        <p className="text-sm text-fg-muted">Loading…</p>
      ) : devices.length === 0 ? (
        <p className="text-sm text-fg-muted">
          No USB devices found. Connect the printer with an OTG cable, switch it on, then Refresh. If nothing appears,
          this phone may not support USB host (OTG) mode.
        </p>
      ) : (
        <ul className="divide-y divide-line rounded-md border border-line">
          {devices.map((device) => {
            const isSelected = selectedId === device.id;
            const isWaiting = awaitingPermission === device.id;
            return (
              <li key={device.id} className="px-3 py-2.5">
                <div className="flex items-center justify-between gap-3">
                  <span className="flex min-w-0 items-center gap-2">
                    <Printer className="h-4 w-4 shrink-0 text-fg-muted" />
                    <span className="min-w-0">
                      <span className="block truncate text-sm font-medium text-fg">{device.name}</span>
                      <span className="flex items-center gap-1.5 text-xs text-fg-muted">
                        {device.id}
                        {device.isPrinterClass && (
                          <span className="rounded bg-surface-3 px-1.5 py-px font-medium text-fg-muted">Printer</span>
                        )}
                      </span>
                    </span>
                  </span>
                  <button
                    type="button"
                    onClick={() => void selectDevice(device)}
                    disabled={isSaving === device.id || isWaiting || isSelected}
                    className={[
                      "shrink-0 rounded-md px-3 py-1.5 text-xs font-semibold",
                      isSelected
                        ? "bg-success-soft text-success-text"
                        : "bg-accent text-accent-ink hover:bg-accent-hover disabled:opacity-50",
                    ].join(" ")}
                  >
                    {isSaving === device.id ? "Saving…" : isSelected ? "Selected" : "Select"}
                  </button>
                </div>

                {isWaiting && (
                  <p className="mt-1.5 flex items-center gap-1.5 text-xs text-fg-muted">
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    Waiting for you to allow access to this device…
                  </p>
                )}

                {!isWaiting && isSelected && !device.hasPermission && (
                  <div className="mt-1.5 flex items-center justify-between gap-3 rounded-md bg-warning-soft px-2 py-1.5">
                    <p className="flex items-start gap-1.5 text-xs text-warning-text">
                      <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                      Android hasn't been allowed to use this device yet. Access is asked for again each time the
                      printer is unplugged and reconnected.
                    </p>
                    <button
                      type="button"
                      onClick={() => void grantAccess(device.id)}
                      className="shrink-0 rounded-md border border-warning-line bg-surface px-2 py-1 text-xs font-semibold text-warning-text hover:bg-warning-soft"
                    >
                      Allow access
                    </button>
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
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
        {error && <ErrorNote message={error} />}
        {selectedName && <SelectedNote label={selectedName} />}

        <ListHeader label="Installed printers" isLoading={isLoading} onRefresh={() => void refresh()} />

        {isLoading ? (
          <p className="text-sm text-fg-muted">Loading…</p>
        ) : printers.length === 0 ? (
          <p className="text-sm text-fg-muted">
            No printers found. Install the thermal printer in Windows (Settings &gt; Printers &amp; scanners) first,
            then Refresh.
          </p>
        ) : (
          <ul className="divide-y divide-line rounded-md border border-line">
            {printers.map((printer) => {
              const isSelected = selectedName === printer.name;
              return (
                <li key={printer.name} className="flex items-center justify-between px-3 py-2.5">
                  <span className="flex min-w-0 items-center gap-2">
                    <Printer className="h-4 w-4 shrink-0 text-fg-muted" />
                    <span className="truncate text-sm font-medium text-fg">{printer.name}</span>
                  </span>
                  <button
                    type="button"
                    onClick={() => void selectPrinter(printer)}
                    disabled={isSaving === printer.name || isSelected}
                    className={[
                      "shrink-0 rounded-md px-3 py-1.5 text-xs font-semibold",
                      isSelected
                        ? "bg-success-soft text-success-text"
                        : "bg-accent text-accent-ink hover:bg-accent-hover disabled:opacity-50",
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
