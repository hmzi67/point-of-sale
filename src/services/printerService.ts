import type { BluetoothDeviceOption, UsbDeviceOption, WindowsPrinterOption } from "../types";
import { call } from "./tauriClient";

/** Every Bluetooth device already paired through the OS settings — the
 * candidate list for Settings' printer picker. Always empty outside
 * Android (USB is auto-detected on macOS/Linux, and Windows has its own
 * picker below — see `PrinterSettingsSection.tsx`). */
export function listBluetoothDevices(): Promise<BluetoothDeviceOption[]> {
  return call<BluetoothDeviceOption[]>("printer_list_bluetooth_devices");
}

/** Every USB device currently attached over OTG — the candidate list for
 * Settings' printer picker in USB mode. Always empty outside Android.
 *
 * Deliberately not filtered to Printer-class devices: many thermal printers
 * (the Gainscha GA-E200 among them) report a vendor-specific interface
 * class, so `isPrinterClass` only sorts and labels the list. */
export function listUsbDevices(): Promise<UsbDeviceOption[]> {
  return call<UsbDeviceOption[]>("printer_list_usb_devices");
}

/** Whether the user has approved this specific attached USB device. Android
 * grants USB access per device, and the grant is dropped on unplug — so
 * unlike `bluetoothPermissionGranted()` this is asked per device rather than
 * once for the app. Always `true` on desktop. */
export function usbPermissionGranted(deviceId: string): Promise<boolean> {
  return call<boolean>("printer_usb_permission_granted", { deviceId });
}

/** Fires Android's system "Allow access to the USB device?" dialog. Same
 * fire-and-forget shape as `requestBluetoothPermission()` — re-check
 * `usbPermissionGranted()` afterwards rather than relying on this call's
 * return to know the outcome. */
export function requestUsbPermission(deviceId: string): Promise<void> {
  return call<void>("printer_request_usb_permission", { deviceId });
}

/** Every printer installed on this machine (the same list "Devices and
 * Printers" would show) — the candidate list for Settings' printer picker
 * on Windows. Always empty elsewhere. */
export function listWindowsPrinters(): Promise<WindowsPrinterOption[]> {
  return call<WindowsPrinterOption[]>("printer_list_windows_printers");
}

/** Whether this app currently holds the Bluetooth permission it needs to
 * list/connect to printers. Always `true` on desktop. */
export function bluetoothPermissionGranted(): Promise<boolean> {
  return call<boolean>("printer_bluetooth_permission_granted");
}

/** Fires the OS Bluetooth-permission dialog if it hasn't been granted yet.
 * Fire-and-forget — re-check `bluetoothPermissionGranted()` afterwards
 * rather than relying on this call's return to know the outcome. */
export function requestBluetoothPermission(): Promise<void> {
  return call<void>("printer_request_bluetooth_permission");
}

/** Prints a ruler + rows of known length at several candidate widths to
 * whichever printer is currently selected — the direct-measurement tool
 * for a printer's real character-per-line width, since that number has
 * turned out to vary by printer model rather than match a datasheet (see
 * `printer::layout`'s module doc comment on the Rust side). Read the
 * printout back: the widest "N=.." line that still prints on one physical
 * line is this printer's real width. */
export function printDiagnostic(): Promise<void> {
  return call<void>("printer_print_diagnostic");
}
