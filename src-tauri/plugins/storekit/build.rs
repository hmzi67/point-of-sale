// No commands are exposed to the webview: the app's own Rust commands call
// this plugin (see `src/lib.rs`), so the frontend never reaches StoreKit
// directly and there are no capability permissions to grant.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();

    // Sign in with Apple needs its entitlement in the app target. This writes
    // it into the generated Xcode project (src-tauri/gen/apple) on every iOS
    // build, so it survives `tauri ios init` regenerating that project — the
    // iOS counterpart of scripts/patch_android_manifest.py. In-App Purchase
    // itself needs no entitlement key, only the capability on the App ID.
    #[cfg(target_os = "macos")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        tauri_plugin::mobile::update_entitlements(|entitlements| {
            entitlements.insert(
                "com.apple.developer.applesignin".into(),
                plist::Value::Array(vec![plist::Value::String("Default".into())]),
            );
        })
        .expect("failed to add the Sign in with Apple entitlement to the iOS project");
    }
}
