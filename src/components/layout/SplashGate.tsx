import { useEffect, useRef, useState } from "react";
import { ping } from "../../services/tauriClient";

const MIN_SPLASH_MS = 400;
const MAX_SPLASH_MS = 3000;
const FADE_OUT_MS = 250;

interface SplashGateProps {
  children: React.ReactNode;
}

/**
 * Android's splash screen. Tauri's native secondary-window splash pattern —
 * a separate "splashscreen" window shown before the main one, see
 * `tauri.conf.json` and `lib.rs`'s `reveal_main_window_after_splash` — isn't
 * available on mobile, which only ever has the one "main" webview (see
 * `tauri.android.conf.json`, which overrides the window list down to just
 * that). So on Android the same fade/scale logo is instead the very first
 * thing the single webview renders, gating the real app tree underneath it
 * — it still appears on every launch with no separate window and no
 * "first launch" tracking needed, since it's just initial render state.
 *
 * Desktop never mounts this at all (see `main.tsx`) — its splash is the
 * real native window, already on screen before this bundle even starts
 * loading.
 *
 * Bounds match the desktop splash: never dismissed before `MIN_SPLASH_MS`
 * (so the fade/scale animation, shared with `public/splashscreen.html`,
 * isn't cut off on fast hardware), never held open past `MAX_SPLASH_MS`
 * regardless of how the readiness check below resolves. `app_ping` stands
 * in for the real signal desktop waits on (`Db::open` completing
 * natively); its failure — or just never resolving — is not a reason to
 * strand the user on the splash, which is what the hard timeout guards.
 */
export function SplashGate({ children }: SplashGateProps) {
  const [visible, setVisible] = useState(true);
  const [fadingOut, setFadingOut] = useState(false);
  const dismissedRef = useRef(false);

  useEffect(() => {
    const start = performance.now();

    const dismiss = () => {
      if (dismissedRef.current) return;
      dismissedRef.current = true;
      setFadingOut(true);
      window.setTimeout(() => setVisible(false), FADE_OUT_MS);
    };

    const waitOutMinimum = () => {
      const remaining = MIN_SPLASH_MS - (performance.now() - start);
      if (remaining <= 0) dismiss();
      else window.setTimeout(dismiss, remaining);
    };

    void ping()
      .catch(() => {
        // Unreachable backend — the hard MAX_SPLASH_MS timeout below still
        // fires regardless, so this never strands the user here.
      })
      .finally(waitOutMinimum);

    const hardStop = window.setTimeout(dismiss, MAX_SPLASH_MS);
    return () => window.clearTimeout(hardStop);
  }, []);

  if (!visible) return <>{children}</>;

  return (
    <div className="h-full">
      {children}
      <div
        className={[
          "fixed inset-0 z-[9999] flex items-center justify-center transition-opacity duration-[250ms] ease-out",
          fadingOut ? "opacity-0" : "opacity-100",
        ].join(" ")}
        // Fixed brand color, not bg-canvas: brand-logo.png is a light/cream
        // card (baked-in background, not transparent — see splashscreen.html's
        // matching comment), so it needs a deliberately dark backdrop for
        // contrast rather than one that tracks the app's own light theme.
        style={{ backgroundColor: "#123326" }}
        aria-hidden="true"
      >
        <img
          src="/brand-logo.png"
          alt=""
          className="h-[min(45vmin,240px)] w-[min(45vmin,240px)] rounded-[20%] shadow-[0_8px_32px_rgba(0,0,0,0.35)] motion-safe:animate-[splash-reveal_500ms_ease-out_forwards]"
        />
      </div>
    </div>
  );
}
