export type BusinessType = "retail" | "restaurant" | "other";

export interface AppConfig {
  businessName: string;
  businessType: BusinessType;
  logoPath: string | null;
  currency: string;
  taxPercent: number;
  receiptFooter: string;
  /** Business contact number, shown in Settings and printed on receipts.
   * `null` until an owner/admin sets one. */
  phone: string | null;
  /** Delivery/dispatch contact number, shown in Settings and printed on
   * receipts (labeled "Delivery: ...") only when set. `null` until an
   * owner/admin sets one. */
  deliveryNumber: string | null;
  /** Set once the first-time setup wizard finishes. `false` on a brand-new
   * install routes the app into onboarding instead of the normal screens. */
  onboardingCompleted: boolean;
  /** The chosen printer transport — set from Settings' "Select printer"
   * step. `null` until that step has actually been done once, which is
   * deliberately indistinguishable from "no printer": printing never
   * guesses at a transport.
   *
   * `"bluetooth"` (Android, a paired device — see `printerBluetoothAddress`)
   * and `"windows"` (an installed printer chosen by name, see
   * `printerWindowsName`) each gate a transport. `"usb"` means two different
   * things by platform, which is safe because nothing dispatches on it
   * without already being on one: on **Android** it names an OTG-attached
   * device in `printerUsbDevice`; on **macOS/Linux** it is informational
   * only, since USB is auto-detected on every print there. */
  printerConnectionType: "usb" | "bluetooth" | "windows" | null;
  /** Paired device MAC address — set only when `printerConnectionType` is
   * `"bluetooth"`. */
  printerBluetoothAddress: string | null;
  /** Paired device display name, stored alongside the address purely so
   * Settings can show "Selected: <name>" without a live Bluetooth query. */
  printerBluetoothName: string | null;
  /** The selected USB printer's `vendor:product` id, lowercase hex (e.g.
   * `"1a2b:3c4d"`) — set only when `printerConnectionType` is `"usb"` on
   * Android. Vendor/product rather than the device's `/dev/bus/usb/...`
   * path because that path is reassigned on every replug, which for an
   * OTG till printer would invalidate the saved selection daily. */
  printerUsbDevice: string | null;
  /** The selected USB printer's display label, stored alongside the id for
   * the same reason `printerBluetoothName` is: so Settings can show the
   * current selection without re-enumerating hardware. */
  printerUsbName: string | null;
  /** The selected Windows printer's name (as `winspool` reports it) — set
   * only when `printerConnectionType` is `"windows"`. This *is* the address
   * on Windows, unlike Bluetooth's separate address/name pair. */
  printerWindowsName: string | null;
}

/** One paired-device entry in Settings' "Select printer" list — see
 * `printerListBluetoothDevices` in `services/printerService.ts`. */
export interface BluetoothDeviceOption {
  name: string;
  address: string;
}

/** One attached USB device in Settings' Android "Select printer" list, in
 * USB mode — see `listUsbDevices` in `services/printerService.ts`. */
export interface UsbDeviceOption {
  /** Display label, e.g. "Gainscha GA-E200". */
  name: string;
  /** `"vendor:product"` in lowercase hex — what gets saved as
   * `AppConfig.printerUsbDevice`. */
  id: string;
  /** Whether the device advertises the standard USB Printer class (0x07).
   * Sorts and labels the list; never filters it — plenty of thermal
   * printers report a vendor-specific class instead, so hiding the rest
   * would hide exactly the hardware this mode exists for. */
  isPrinterClass: boolean;
  /** Whether the user has already approved this specific device. Android's
   * USB grant is per-device and doesn't survive an unplug, so this belongs
   * to a row rather than to the app the way Bluetooth's permission does. */
  hasPermission: boolean;
}

/** One installed printer in Settings' Windows "Select printer" list — see
 * `printerListWindowsPrinters` in `services/printerService.ts`. */
export interface WindowsPrinterOption {
  name: string;
}
