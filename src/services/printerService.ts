import type { BluetoothDeviceOption } from "../types";
import { call } from "./tauriClient";

/** Every Bluetooth device already paired through the OS settings — the
 * candidate list for Settings' printer picker. Always empty on desktop
 * (USB is auto-detected there — see `PrinterSettingsSection.tsx`). */
export function listBluetoothDevices(): Promise<BluetoothDeviceOption[]> {
  return call<BluetoothDeviceOption[]>("printer_list_bluetooth_devices");
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
