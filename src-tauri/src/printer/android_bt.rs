//! Android Bluetooth Classic (SPP) printer transport.
//!
//! Talks to `android.bluetooth.*` directly via JNI rather than through a
//! full Tauri mobile plugin (a separate crate + Kotlin module) — everything
//! this needs (list bonded devices, connect, write bytes, check permission)
//! is a handful of standard-library Android SDK calls with no UI of their
//! own, so the extra plugin scaffolding wasn't worth it.
//!
//! Getting a `JavaVM`/`Context` without Tauri's cooperation (its own JNI
//! plumbing — `AppHandle::runtime()`, `run_on_android_context` — is
//! `pub(crate)`-sealed, unreachable from application code, and this crate
//! deliberately isn't a full Tauri plugin, the only other thing that gets
//! it) took two false starts before landing on [`JNI_OnLoad`], the one
//! genuinely reliable mechanism:
//!
//! 1. `ndk_context::android_context()` — the usual answer for a winit-family
//!    app — panics "android context was not initialized" on a real device:
//!    nothing in this Tauri version's actual Android dependency chain calls
//!    `ndk_context::initialize_android_context` at all.
//! 2. `JNI_GetCreatedJavaVMs` (the "just ask for any VM in this process"
//!    invocation-API call, which works fine on desktop JVMs) doesn't even
//!    link on Android — the library fails to load at all with
//!    `UnsatisfiedLinkError: dlopen failed: cannot locate symbol
//!    "JNI_GetCreatedJavaVMs"`, which is worse than either bug above: it
//!    takes the *entire app* down on every launch, not just this feature.
//!
//! [`JNI_OnLoad`] is different: it's a guarantee of the JNI specification
//! itself (any native library loaded via `System.loadLibrary` gets it
//! called automatically, with a live `JavaVM*`, the moment the library
//! loads) — not something Tauri/tao/`ndk-context` need to have specifically
//! wired up for this to work. It stores the `JavaVM` in the static
//! [`JAVA_VM`]; everything else in this module reads that.
//!
//! For the `Context` half, `android.app.ActivityThread.currentApplication()`
//! — a long-stable, widely-used-by-libraries reflection call — returns the
//! app's `Application`, which is sufficient for every call this module
//! makes *except* requesting a runtime permission
//! (`ActivityCompat.requestPermissions` needs a real `Activity`, which isn't
//! obtainable this way) — see `request_permission`'s doc comment for how
//! that one's designed around not needing one either.
//!
//! Deliberately connect-only, no discovery/scan: this only reaches devices
//! already paired through the OS Bluetooth settings (`getBondedDevices`),
//! which only needs `BLUETOOTH_CONNECT` (API 31+) — scanning for *new*
//! devices would also need `BLUETOOTH_SCAN` and, on pre-12 Android, a
//! location permission, for a use case (a shop's one till printer) that's
//! always already paired anyway.
//!
//! Every JNI call here returns `jni::errors::Result` and is propagated with
//! `?` — nothing is `.unwrap()`/`.expect()`d. Any pending Java exception
//! (e.g. `SecurityException` from a missing permission) is explicitly
//! cleared before returning our own error, so a failure here can never
//! leave a dangling exception to trip up unrelated JNI calls elsewhere in
//! the app afterwards. This is deliberately the *only* place in the app
//! that touches JNI directly — see `Cargo.toml`'s comment on why USB (the
//! previous Android attempt, via `rusb`/libusb) was dropped instead of
//! fixed: hand-written JNI is real hardware I/O and can't be made
//! panic-proof against a genuinely wrong method signature the way pure
//! Rust can, so keeping that surface as small and single-purpose as
//! possible is the actual safety measure, not a nice-to-have.

use crate::printer::escpos::PrinterError;
use jni::objects::{JObject, JString, JValue};
use jni::JNIEnv;

const BLUETOOTH_CONNECT_PERMISSION: &str = "android.permission.BLUETOOTH_CONNECT";
/// The standard Serial Port Profile UUID — what makes a Bluetooth Classic
/// socket a plain byte stream, which is all an ESC/POS printer needs.
const SPP_UUID: &str = "00001101-0000-1000-8000-00805F9B34FB";

pub struct BluetoothDeviceInfo {
    pub name: String,
    pub address: String,
}

/// The app's one JVM, captured by [`jni_on_load`] the moment Android loads
/// this library — the only reliable way to get it. `JNI_GetCreatedJavaVMs`
/// (the "just ask the JVM for any VM in this process" invocation-API call
/// that works on desktop JVMs) was tried first and doesn't work on Android
/// at all: it isn't a symbol the dynamic linker can resolve there, so a
/// build calling it doesn't even get as far as running — the library fails
/// to load with `UnsatisfiedLinkError: dlopen failed: cannot locate symbol
/// "JNI_GetCreatedJavaVMs"` (confirmed on a real device), taking the whole
/// app down before any of its own code runs, not just this module's.
/// `JavaVM` is `Send + Sync` (attaching per-thread is its whole purpose),
/// so a `'static` reference here is safe to use from any Tauri command's
/// worker thread.
static JAVA_VM: std::sync::OnceLock<jni::JavaVM> = std::sync::OnceLock::new();

/// The one JNI entry point every native library gets called on automatically
/// by the JVM immediately after `System.loadLibrary` loads it — a guarantee
/// of the JNI specification itself, not something Tauri needs to
/// opt into or cooperate with (unlike `ndk-context`, which nothing in this
/// Tauri version's Android runtime turns out to populate — see this
/// module's doc comment). Exported via `#[no_mangle]`/the standard
/// `JNI_OnLoad` name so the JVM finds it without any registration.
#[no_mangle]
pub unsafe extern "system" fn JNI_OnLoad(vm: *mut jni::sys::JavaVM, _reserved: *mut std::ffi::c_void) -> jni::sys::jint {
    match unsafe { jni::JavaVM::from_raw(vm) } {
        Ok(java_vm) => {
            let _ = JAVA_VM.set(java_vm); // already set is fine — JNI_OnLoad only ever fires once in practice
            jni::sys::JNI_VERSION_1_6
        }
        Err(_) => jni::sys::JNI_ERR,
    }
}

/// Attaches the current thread to [`JAVA_VM`] and runs `f` with the
/// resulting `JNIEnv` and the app's `Application` object (a valid `Context`
/// for everything except requesting a permission) — a callback rather than
/// returning the attached environment directly because `AttachGuard`
/// borrows from the `JavaVM` reference. A fresh attach per call rather than
/// a cached one, since
/// Tauri commands can run on any thread pool worker and JNI attachment is
/// per-thread.
fn with_env<T>(f: impl FnOnce(&mut JNIEnv, &JObject) -> Result<T, PrinterError>) -> Result<T, PrinterError> {
    let vm = JAVA_VM.get().ok_or_else(|| {
        PrinterError::Io("Android JNI unavailable — the app's JNI_OnLoad hasn't run yet".to_string())
    })?;
    let mut env = vm.attach_current_thread().map_err(|e| PrinterError::Io(format!("JNI attach failed: {e}")))?;

    let activity_thread_class = env
        .find_class("android/app/ActivityThread")
        .map_err(|e| io_err(&mut env, "ActivityThread lookup failed", e))?;
    let context = env
        .call_static_method(activity_thread_class, "currentApplication", "()Landroid/app/Application;", &[])
        .and_then(|v| v.l())
        .map_err(|e| io_err(&mut env, "currentApplication failed", e))?;
    if context.is_null() {
        return Err(PrinterError::Io("Android application context unavailable".to_string()));
    }

    f(&mut env, &context)
}

/// Clears whatever Java exception is currently pending (if any) so it can't
/// affect the next unrelated JNI call — see this module's doc comment.
fn clear_exception(env: &mut JNIEnv) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

fn io_err(env: &mut JNIEnv, context: &str, e: jni::errors::Error) -> PrinterError {
    clear_exception(env);
    PrinterError::Io(format!("{context}: {e}"))
}

/// `android.os.Build.VERSION.SDK_INT` — the runtime Bluetooth permission
/// only exists (and is only enforced) from API 31 (Android 12) onward; on
/// older versions `BLUETOOTH`/`BLUETOOTH_ADMIN` are install-time manifest
/// permissions the OS grants automatically, so there is nothing to check or
/// request there.
fn sdk_int(env: &mut JNIEnv) -> Result<i32, PrinterError> {
    let class = env
        .find_class("android/os/Build$VERSION")
        .map_err(|e| io_err(env, "Build.VERSION lookup failed", e))?;
    env.get_static_field(class, "SDK_INT", "I")
        .and_then(|v| v.i())
        .map_err(|e| io_err(env, "SDK_INT read failed", e))
}

/// Whether this app currently holds `BLUETOOTH_CONNECT` — always `true` on
/// API < 31 (see [`sdk_int`]'s doc comment).
///
/// Calls the plain framework `Context.checkSelfPermission` (added in API 23,
/// this app's `minSdk`), not `androidx.core.content.ContextCompat`'s
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
/// obtainable through `with_env`'s `Application`-context approach (see this
/// module's doc comment); (2) even with one, its result is asynchronous
/// (`onRequestPermissionsResult`, delivered on the Activity's UI thread),
/// and synchronously bridging that back to whichever background thread a
/// Tauri command runs on would need either fragile polling or a custom
/// `MainActivity` override wired to a static channel — real, but
/// meaningfully more moving parts than "open Settings, let the user flip it,
/// re-check when they come back", which needs neither. The Settings screen
/// re-checks [`permission_granted`] itself when it regains focus — see
/// `PrinterSettingsSection.tsx`.
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
        // (here, the Application context — see this module's doc comment).
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

fn jstring_to_string(env: &mut JNIEnv, obj: JObject) -> Result<String, PrinterError> {
    let jstr = JString::from(obj);
    let s = env.get_string(&jstr).map_err(|e| io_err(env, "string decode failed", e))?;
    Ok(s.into())
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

        let enabled = env
            .call_method(&adapter, "isEnabled", "()Z", &[])
            .and_then(|v| v.z())
            .map_err(|e| io_err(env, "isEnabled check failed", e))?;
        if !enabled {
            return Err(PrinterError::Io("Bluetooth is turned off".into()));
        }

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
        let enabled = env
            .call_method(&adapter, "isEnabled", "()Z", &[])
            .and_then(|v| v.z())
            .map_err(|e| io_err(env, "isEnabled check failed", e))?;
        if !enabled {
            return Err(PrinterError::Io("Bluetooth is turned off".into()));
        }

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
