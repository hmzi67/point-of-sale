//! Android Bluetooth Classic (SPP) printer transport — one of the two
//! Android transports, alongside `android_usb` (USB-OTG). Which one a given
//! install uses is `AppConfig.printer_connection_type`, chosen in Settings;
//! see `printer::escpos::send_to_printer_dispatch`.
//!
//! Talks to `android.bluetooth.*` directly via JNI rather than through a
//! full Tauri mobile plugin (a separate crate + Kotlin module) — everything
//! this needs (list bonded devices, connect, write bytes, check permission)
//! is a handful of standard-library Android SDK calls with no UI of their
//! own, so the extra plugin scaffolding wasn't worth it. The `JavaVM`/
//! `Context` plumbing that makes those calls possible at all is shared with
//! `android_usb` and lives in `printer::android_jni` — read that module's
//! doc comment first; it explains why `JNI_OnLoad` is the only mechanism
//! that works here.
//!
//! Deliberately connect-only, no discovery/scan: this only reaches devices
//! already paired through the OS Bluetooth settings (`getBondedDevices`),
//! which only needs `BLUETOOTH_CONNECT` (API 31+) — scanning for *new*
//! devices would also need `BLUETOOTH_SCAN` and, on pre-12 Android, a
//! location permission, for a use case (a shop's one till printer) that's
//! always already paired anyway.
//!
//! **The permissions this needs must be declared in `AndroidManifest.xml`**
//! (`BLUETOOTH`/`BLUETOOTH_ADMIN` with `maxSdkVersion="30"`, plus
//! `BLUETOOTH_CONNECT` for API 31+). They were missing for a while, and the
//! symptom was not the obvious "permission denied" — [`permission_granted`]
//! returns `true` on API < 31 without checking anything (correctly: there
//! is no runtime permission to check there), so the first call that
//! actually touched the radio, `BluetoothAdapter.isEnabled()`, threw a bare
//! `SecurityException` surfacing as "isEnabled check failed: Java
//! exception". `tauri android init` regenerates the manifest from scratch
//! and `gen/android` is gitignored, so the declarations are (re)applied by
//! `scripts/patch_android_manifest.py` — run it after any `android init`,
//! exactly like `patch_android_signing.py`.

use crate::printer::android_jni::{io_err, jstring_to_string, sdk_int, with_env};
use crate::printer::escpos::PrinterError;
use jni::objects::{JObject, JValue};
use jni::JNIEnv;

const BLUETOOTH_CONNECT_PERMISSION: &str = "android.permission.BLUETOOTH_CONNECT";
/// The standard Serial Port Profile UUID — what makes a Bluetooth Classic
/// socket a plain byte stream, which is all an ESC/POS printer needs.
const SPP_UUID: &str = "00001101-0000-1000-8000-00805F9B34FB";

pub struct BluetoothDeviceInfo {
    pub name: String,
    pub address: String,
}

/// Whether this app currently holds `BLUETOOTH_CONNECT` — always `true` on
/// API < 31, where `BLUETOOTH`/`BLUETOOTH_ADMIN` are install-time manifest
/// permissions the OS grants automatically and there is nothing to request.
/// Note that "nothing to request" is not "nothing to declare": both still
/// have to be in the manifest, and their absence is what produced the
/// long-standing `isEnabled` `SecurityException` — see this module's doc
/// comment.
///
/// Calls the plain framework `Context.checkSelfPermission` (added in API 23,
/// below this app's `minSdk` of 24), not `androidx.core.content.ContextCompat`'s
/// identical-behavior wrapper — deliberately, after that version threw on a
/// real device's *release* build specifically: R8 had stripped
/// `ContextCompat` as an apparently-unused class, since nothing in the
/// Kotlin/Java side references it — only this JNI reflection call does,
/// invisible to R8's reachability analysis. The framework `Context` class
/// itself is never a minification target, so calling straight through it
/// sidesteps that whole failure mode rather than needing a ProGuard keep
/// rule to paper over it.
pub fn permission_granted() -> Result<bool, PrinterError> {
    with_env(|env, context| {
        if sdk_int(env)? < 31 {
            return Ok(true);
        }

        let permission = env
            .new_string(BLUETOOTH_CONNECT_PERMISSION)
            .map_err(|e| io_err(env, "permission string alloc failed", e))?;
        let result = env
            .call_method(context, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&permission)])
            .and_then(|v| v.i())
            .map_err(|e| io_err(env, "checkSelfPermission failed", e))?;

        const PERMISSION_GRANTED: i32 = 0; // android.content.pm.PackageManager.PERMISSION_GRANTED
        Ok(result == PERMISSION_GRANTED)
    })
}

/// Opens this app's system Settings screen (API 31+ and not already
/// granted; a no-op otherwise) so the user can grant the Bluetooth
/// permission there, rather than firing `ActivityCompat.requestPermissions`'
/// in-app dialog directly. Two independent reasons this is the better fit
/// here, not just a fallback: (1) `requestPermissions` needs a real
/// `Activity`, which — unlike everything else in this module — isn't
/// obtainable through `android_jni::with_env`'s `Application`-context
/// approach (see that module's doc comment); (2) even with one, its result is
/// asynchronous (`onRequestPermissionsResult`, delivered on the Activity's UI
/// thread), and synchronously bridging that back to whichever background
/// thread a Tauri command runs on would need either fragile polling or a
/// custom `MainActivity` override wired to a static channel — real, but
/// meaningfully more moving parts than "open Settings, let the user flip it,
/// re-check when they come back", which needs neither. The Settings screen
/// re-checks [`permission_granted`] itself when it regains focus — see
/// `PrinterSettingsSection.tsx`.
///
/// (`android_usb`'s equivalent *can* use the real in-app dialog, because
/// `UsbManager.requestPermission` takes a `PendingIntent` rather than
/// needing an `Activity` — see [`crate::printer::android_usb::request_permission`].)
pub fn request_permission() -> Result<(), PrinterError> {
    if permission_granted()? {
        return Ok(());
    }

    with_env(|env, context| {
        if sdk_int(env)? < 31 {
            return Ok(());
        }

        let package_name = env
            .call_method(context, "getPackageName", "()Ljava/lang/String;", &[])
            .and_then(|v| v.l())
            .map_err(|e| io_err(env, "getPackageName failed", e))?;

        let scheme = env.new_string("package").map_err(|e| io_err(env, "scheme string alloc failed", e))?;
        let uri = env
            .call_static_method(
                "android/net/Uri",
                "fromParts",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Landroid/net/Uri;",
                &[JValue::Object(&scheme), JValue::Object(&package_name), JValue::Object(&JObject::null())],
            )
            .and_then(|v| v.l())
            .map_err(|e| io_err(env, "Uri.fromParts failed", e))?;

        let action = env
            .new_string("android.settings.APPLICATION_DETAILS_SETTINGS")
            .map_err(|e| io_err(env, "action string alloc failed", e))?;
        let intent_class = env.find_class("android/content/Intent").map_err(|e| io_err(env, "Intent lookup failed", e))?;
        let intent = env
            .new_object(
                intent_class,
                "(Ljava/lang/String;Landroid/net/Uri;)V",
                &[JValue::Object(&action), JValue::Object(&uri)],
            )
            .map_err(|e| io_err(env, "Intent construction failed", e))?;

        // Required when starting an Activity from a non-Activity Context
        // (here, the Application context — see `android_jni`'s doc comment).
        const FLAG_ACTIVITY_NEW_TASK: i32 = 0x1000_0000;
        env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)])
            .map_err(|e| io_err(env, "addFlags failed", e))?;

        env.call_method(context, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])
            .map(|_| ())
            .map_err(|e| io_err(env, "startActivity failed", e))
    })
}

/// `BluetoothAdapter.getDefaultAdapter()` — `None` means this device has no
/// Bluetooth radio at all (some tablets/POS terminals), not a permission or
/// pairing problem.
fn default_adapter<'a>(env: &mut JNIEnv<'a>) -> Result<Option<JObject<'a>>, PrinterError> {
    let class = env
        .find_class("android/bluetooth/BluetoothAdapter")
        .map_err(|e| io_err(env, "BluetoothAdapter lookup failed", e))?;
    let adapter = env
        .call_static_method(
            class,
            "getDefaultAdapter",
            "()Landroid/bluetooth/BluetoothAdapter;",
            &[],
        )
        .and_then(|v| v.l())
        .map_err(|e| io_err(env, "getDefaultAdapter failed", e))?;
    Ok(if adapter.is_null() { None } else { Some(adapter) })
}

/// `BluetoothAdapter.isEnabled()`, with the one failure mode that isn't
/// "the radio is off" spelled out.
///
/// This call needs `BLUETOOTH` (API < 31) or `BLUETOOTH_CONNECT` (31+) to be
/// *declared in the manifest*, and throws a bare `SecurityException` if it
/// isn't — which JNI surfaces as an opaque "Java exception" with nothing in
/// it pointing at the cause. That was the exact shape of a real bug here (see
/// this module's doc comment), so the error text says what to check rather
/// than passing the opaque one straight through.
fn require_enabled(env: &mut JNIEnv, adapter: &JObject) -> Result<(), PrinterError> {
    let enabled = env.call_method(adapter, "isEnabled", "()Z", &[]).and_then(|v| v.z()).map_err(|e| {
        io_err(
            env,
            "couldn't read the Bluetooth adapter state — the app may be missing the Bluetooth permission",
            e,
        )
    })?;
    if !enabled {
        return Err(PrinterError::Io("Bluetooth is turned off".into()));
    }
    Ok(())
}

/// Every device already paired through the OS Bluetooth settings — the
/// candidate list for Settings' "Select printer" step. Requires
/// `BLUETOOTH_CONNECT` on API 31+ (checked up front, not left to surface as
/// an opaque `SecurityException` from `getBondedDevices` itself).
pub fn list_bonded_devices() -> Result<Vec<BluetoothDeviceInfo>, PrinterError> {
    if !permission_granted()? {
        return Err(PrinterError::PermissionDenied);
    }

    with_env(|env, _activity| {
        let Some(adapter) = default_adapter(env)? else {
            return Err(PrinterError::Io("This device has no Bluetooth hardware".into()));
        };

        require_enabled(env, &adapter)?;

        let bonded_set = env
            .call_method(&adapter, "getBondedDevices", "()Ljava/util/Set;", &[])
            .and_then(|v| v.l())
            .map_err(|e| io_err(env, "getBondedDevices failed", e))?;
        let bonded_array = env
            .call_method(&bonded_set, "toArray", "()[Ljava/lang/Object;", &[])
            .and_then(|v| v.l())
            .map_err(|e| io_err(env, "bonded-set toArray failed", e))?;
        let bonded_array = jni::objects::JObjectArray::from(bonded_array);

        let len = env.get_array_length(&bonded_array).map_err(|e| io_err(env, "array length read failed", e))?;

        let mut devices = Vec::with_capacity(len as usize);
        for i in 0..len {
            let device = env
                .get_object_array_element(&bonded_array, i)
                .map_err(|e| io_err(env, "array element read failed", e))?;

            let name_obj = env
                .call_method(&device, "getName", "()Ljava/lang/String;", &[])
                .and_then(|v| v.l())
                .map_err(|e| io_err(env, "getName failed", e))?;
            let address_obj = env
                .call_method(&device, "getAddress", "()Ljava/lang/String;", &[])
                .and_then(|v| v.l())
                .map_err(|e| io_err(env, "getAddress failed", e))?;

            let name =
                if name_obj.is_null() { "(unnamed device)".to_string() } else { jstring_to_string(env, name_obj)? };
            let address = jstring_to_string(env, address_obj)?;
            devices.push(BluetoothDeviceInfo { name, address });
        }

        Ok(devices)
    })
}

/// Connects to the already-paired device at `address` over RFCOMM/SPP,
/// writes `bytes`, and closes the socket — one shot per print, no
/// persistent connection kept around between sales (a receipt is small and
/// infrequent enough that reconnect overhead is not worth the complexity
/// and failure modes of managing a long-lived socket).
pub fn send(address: &str, bytes: &[u8]) -> Result<(), PrinterError> {
    if !permission_granted()? {
        return Err(PrinterError::PermissionDenied);
    }

    with_env(|env, _activity| {
        let Some(adapter) = default_adapter(env)? else {
            return Err(PrinterError::Io("This device has no Bluetooth hardware".into()));
        };
        require_enabled(env, &adapter)?;

        let address_jstr = env.new_string(address).map_err(|e| io_err(env, "address string alloc failed", e))?;
        let device = env
            .call_method(
                &adapter,
                "getRemoteDevice",
                "(Ljava/lang/String;)Landroid/bluetooth/BluetoothDevice;",
                &[JValue::Object(&address_jstr)],
            )
            .and_then(|v| v.l())
            .map_err(|e| io_err(env, "getRemoteDevice failed — is the printer still paired?", e))?;

        let uuid_jstr = env.new_string(SPP_UUID).map_err(|e| io_err(env, "UUID string alloc failed", e))?;
        let uuid = env
            .call_static_method(
                "java/util/UUID",
                "fromString",
                "(Ljava/lang/String;)Ljava/util/UUID;",
                &[JValue::Object(&uuid_jstr)],
            )
            .and_then(|v| v.l())
            .map_err(|e| io_err(env, "UUID.fromString failed", e))?;

        let socket = env
            .call_method(
                &device,
                "createRfcommSocketToServiceRecord",
                "(Ljava/util/UUID;)Landroid/bluetooth/BluetoothSocket;",
                &[JValue::Object(&uuid)],
            )
            .and_then(|v| v.l())
            .map_err(|e| io_err(env, "socket creation failed", e))?;

        // `connect()` blocks until the printer accepts (or the OS times
        // out) — that's the point: this whole function is meant to run off
        // the UI thread (a Tauri command already does), and there's nothing
        // useful to do while waiting other than wait.
        if let Err(e) = env.call_method(&socket, "connect", "()V", &[]) {
            let err = io_err(env, "couldn't connect — check the printer is on and in range", e);
            let _ = env.call_method(&socket, "close", "()V", &[]);
            return Err(err);
        }

        let out_stream = match env
            .call_method(&socket, "getOutputStream", "()Ljava/io/OutputStream;", &[])
            .and_then(|v| v.l())
        {
            Ok(s) => s,
            Err(e) => {
                let err = io_err(env, "couldn't open the printer's output stream", e);
                let _ = env.call_method(&socket, "close", "()V", &[]);
                return Err(err);
            }
        };

        let byte_array = match env.byte_array_from_slice(bytes) {
            Ok(a) => a,
            Err(e) => {
                let err = io_err(env, "byte array alloc failed", e);
                let _ = env.call_method(&socket, "close", "()V", &[]);
                return Err(err);
            }
        };

        let write_result = env
            .call_method(&out_stream, "write", "([B)V", &[JValue::Object(&byte_array)])
            .and_then(|_| env.call_method(&out_stream, "flush", "()V", &[]));

        let _ = env.call_method(&socket, "close", "()V", &[]);

        write_result.map(|_| ()).map_err(|e| io_err(env, "printer write failed mid-receipt", e))
    })
}
