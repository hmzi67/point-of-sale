//! The shared Android JNI plumbing both Android printer transports need:
//! a `JavaVM`, a `Context`, and the error/exception discipline around them.
//!
//! This lives in its own module rather than inside either transport because
//! [`JNI_OnLoad`] can only be defined *once* per native library — the JVM
//! looks it up by that exact exported symbol — so the moment a second
//! transport (`android_usb`, alongside `android_bt`) also needed a
//! `JavaVM`, "each module captures its own" stopped being an option. Both
//! now read the one [`JAVA_VM`] captured here.
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
//! wired up for this to work.
//!
//! For the `Context` half, `android.app.ActivityThread.currentApplication()`
//! — a long-stable, widely-used-by-libraries reflection call — returns the
//! app's `Application`, which is sufficient for every call either transport
//! makes *except* requesting a runtime permission
//! (`ActivityCompat.requestPermissions` needs a real `Activity`, which isn't
//! obtainable this way) — see `android_bt::request_permission`'s doc comment
//! for how that one's designed around not needing one either.
//!
//! Every JNI call in this module and its two callers returns
//! `jni::errors::Result` and is propagated with `?` — nothing is
//! `.unwrap()`/`.expect()`d. Any pending Java exception (e.g. a
//! `SecurityException` from a missing permission) is explicitly cleared by
//! [`io_err`] before returning our own error, so a failure can never leave a
//! dangling exception to trip up unrelated JNI calls elsewhere in the app
//! afterwards. This module plus `android_bt`/`android_usb` are deliberately
//! the *only* place in the app that touches JNI directly — see `Cargo.toml`'s
//! comment on why `rusb` (the previous Android USB attempt, via libusb) was
//! dropped rather than fixed: hand-written JNI is real hardware I/O and
//! can't be made panic-proof against a genuinely wrong method signature the
//! way pure Rust can, so keeping that surface as small and single-purpose as
//! possible is the actual safety measure, not a nice-to-have. (The
//! replacement for that dropped libusb path is `android_usb`, which goes
//! through Android's own `UsbManager` — the permission model libusb was
//! never designed for is exactly what `UsbManager` exists to implement.)

use crate::printer::escpos::PrinterError;
use jni::objects::JObject;
use jni::JNIEnv;

/// The app's one JVM, captured by [`JNI_OnLoad`] the moment Android loads
/// this library — the only reliable way to get it (see this module's doc
/// comment for the two approaches that don't work). `JavaVM` is
/// `Send + Sync` (attaching per-thread is its whole purpose), so a
/// `'static` reference here is safe to use from any Tauri command's worker
/// thread.
static JAVA_VM: std::sync::OnceLock<jni::JavaVM> = std::sync::OnceLock::new();

/// The one JNI entry point every native library gets called on automatically
/// by the JVM immediately after `System.loadLibrary` loads it — a guarantee
/// of the JNI specification itself, not something Tauri needs to opt into or
/// cooperate with. Exported via `#[no_mangle]`/the standard `JNI_OnLoad`
/// name so the JVM finds it without any registration. There can only be one
/// of these in the whole library, which is why it lives here rather than in
/// either transport module.
#[no_mangle]
pub unsafe extern "system" fn JNI_OnLoad(
    vm: *mut jni::sys::JavaVM,
    _reserved: *mut std::ffi::c_void,
) -> jni::sys::jint {
    match unsafe { jni::JavaVM::from_raw(vm) } {
        Ok(java_vm) => {
            let _ = JAVA_VM.set(java_vm); // already set is fine — JNI_OnLoad only ever fires once in practice
            jni::sys::JNI_VERSION_1_6
        }
        Err(_) => jni::sys::JNI_ERR,
    }
}

/// The raw `JavaVM*` captured by [`JNI_OnLoad`], for code that has to hand
/// it to a crate built on a different `jni` major version (the TLS
/// verifier — see `license::android_tls`).
pub fn raw_java_vm() -> Option<*mut jni::sys::JavaVM> {
    JAVA_VM.get().map(|vm| vm.get_java_vm_pointer())
}

/// Attaches the current thread to [`JAVA_VM`] and runs `f` with the
/// resulting `JNIEnv` and the app's `Application` object (a valid `Context`
/// for everything except requesting a permission) — a callback rather than
/// returning the attached environment directly because `AttachGuard`
/// borrows from the `JavaVM` reference. A fresh attach per call rather than
/// a cached one, since Tauri commands can run on any thread pool worker and
/// JNI attachment is per-thread.
pub fn with_env<T>(
    f: impl FnOnce(&mut JNIEnv, &JObject) -> Result<T, PrinterError>,
) -> Result<T, PrinterError> {
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
pub fn clear_exception(env: &mut JNIEnv) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

/// Turns a JNI error into a [`PrinterError::Io`] with `context` prefixed,
/// clearing any pending Java exception on the way out.
pub fn io_err(env: &mut JNIEnv, context: &str, e: jni::errors::Error) -> PrinterError {
    clear_exception(env);
    PrinterError::Io(format!("{context}: {e}"))
}

/// `android.os.Build.VERSION.SDK_INT` — several permission behaviours below
/// only exist (and are only enforced) from a particular API level onward, so
/// both transports need to branch on it.
pub fn sdk_int(env: &mut JNIEnv) -> Result<i32, PrinterError> {
    let class = env
        .find_class("android/os/Build$VERSION")
        .map_err(|e| io_err(env, "Build.VERSION lookup failed", e))?;
    env.get_static_field(class, "SDK_INT", "I")
        .and_then(|v| v.i())
        .map_err(|e| io_err(env, "SDK_INT read failed", e))
}

/// Decodes a `java.lang.String` object (arriving as a plain [`JObject`], as
/// every `call_method(...).l()` does) into a Rust `String`.
pub fn jstring_to_string(env: &mut JNIEnv, obj: JObject) -> Result<String, PrinterError> {
    let jstr = jni::objects::JString::from(obj);
    let s = env.get_string(&jstr).map_err(|e| io_err(env, "string decode failed", e))?;
    Ok(s.into())
}
