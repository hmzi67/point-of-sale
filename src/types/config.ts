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
   * guesses at a transport. `"usb"` (macOS/Linux) is informational only —
   * USB is auto-detected either way; `"bluetooth"` (Android) and
   * `"windows"` (an installed printer chosen by name, see
   * `printerWindowsName`) are the ones that actually gate anything. */
  printerConnectionType: "usb" | "bluetooth" | "windows" | null;
  /** Paired device MAC address — set only when `printerConnectionType` is
   * `"bluetooth"`. */
  printerBluetoothAddress: string | null;
  /** Paired device display name, stored alongside the address purely so
   * Settings can show "Selected: <name>" without a live Bluetooth query. */
  printerBluetoothName: string | null;
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

/** One installed printer in Settings' Windows "Select printer" list — see
 * `printerListWindowsPrinters` in `services/printerService.ts`. */
export interface WindowsPrinterOption {
  name: string;
}
