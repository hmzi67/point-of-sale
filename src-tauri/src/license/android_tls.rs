//! One-time Android initialisation of `rustls-platform-verifier`, the
//! certificate verifier reqwest uses for the licence server's HTTPS.
//!
//! On Android that crate verifies through the OS trust manager via JNI, so
//! it must be given the JVM and the app `Context` before its first use;
//! without that it panics ("Expect rustls-platform-verifier to be
//! initialized") inside the request. The panic took down the async
//! `license_activate` command before it could return, which is why entering
//! a key on Android spun and never activated.
//!
//! The verifier's init API is built on `jni` 0.22 while the rest of the app
//! (and `printer::android_jni`) uses 0.21, so this bridges the two: the
//! 0.21 plumbing supplies the captured `JavaVM*` and the `Application`
//! context (as a global ref, valid on any thread), and a 0.22 `JavaVM`
//! wraps the same raw pointer to make the call. The Kotlin half of the
//! verifier is added to the Gradle project by
//! `scripts/patch_android_tls_verifier.py`.

use std::sync::OnceLock;

use crate::printer::android_jni;

/// Initialises at most once per process; a failure is remembered and
/// reported on every later call rather than retried in a loop.
pub fn ensure_initialized() -> Result<(), String> {
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();
    INIT.get_or_init(|| {
        let result = init();
        if let Err(e) = &result {
            crate::startup_log::log(&format!("license: TLS verifier init failed: {e}"));
        }
        result
    })
    .clone()
}

fn init() -> Result<(), String> {
    let raw_vm = android_jni::raw_java_vm().ok_or("secure connection unavailable (JNI not loaded)")?;

    // A global ref so the context survives the 0.21 attach guard below
    // detaching this thread; the verifier takes its own global ref from it.
    let context = android_jni::with_env(|env, context| {
        env.new_global_ref(context).map_err(|e| android_jni::io_err(env, "context ref failed", e))
    })
    .map_err(|e| format!("secure connection unavailable ({e})"))?;

    // SAFETY: `raw_vm` is the live JavaVM captured by JNI_OnLoad, which
    // lasts for the life of the process.
    let vm = unsafe { jni022::JavaVM::from_raw(raw_vm.cast()) };
    vm.attach_current_thread(|env| -> Result<(), jni022::errors::Error> {
        // SAFETY: a valid global reference to the Application object, kept
        // alive by `context` for the whole call. `JObject` has no Drop, so
        // wrapping it never deletes the reference.
        let context_obj = unsafe { jni022::objects::JObject::from_raw(env, context.as_obj().as_raw().cast()) };
        rustls_platform_verifier::android::init_with_env(env, context_obj)
    })
    .map_err(|e| format!("secure connection unavailable ({e})"))
}
