import { useState } from "react";
import { IS_ANDROID } from "../../services/platform";
import { BottomTabBar } from "./BottomTabBar";
import { DefaultPinBanner } from "./DefaultPinBanner";
import { LicenseBanner } from "./LicenseBanner";
import { LicenseLockGate } from "./LicenseLockGate";
import { NavOverlay } from "./NavOverlay";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";

/** Navigation is platform-split here and nowhere else:
 *
 * - **Desktop** gets the attached, collapsible `Sidebar` on the left, which
 *   pushes the content area rather than overlaying it. No hamburger — the
 *   rail's own collapse toggle is the "more room" control.
 * - **Android** keeps the hamburger-triggered `NavOverlay` (this is the one
 *   place its open/closed state lives, so the button in `TopBar` and the
 *   drawer can be siblings without prop-drilling) plus the fixed
 *   `BottomTabBar` for primary modules. A persistent left rail on a
 *   phone-width screen would eat too much of it.
 *
 * Both render the same `useNavItems()` list, so the platforms can't
 * disagree about what a role sees. */
export function AppLayout() {
  const [navOpen, setNavOpen] = useState(false);

  return (
    <div className="flex h-full bg-canvas">
      {!IS_ANDROID && <Sidebar />}

      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar onOpenNav={IS_ANDROID ? () => setNavOpen(true) : undefined} />
        {/* Between the bar and the page so it's on every screen and never
            covers the page's own header. Renders nothing unless the licence
            is inside its expiry-warning window. */}
        <LicenseBanner />
        <DefaultPinBanner />
        <main className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto p-2.5 sm:p-4 lg:p-5">
          {/* Replaces every module screen with the lock screen while the
              licence is not OK; Settings stays open for Owner/Admin. */}
          <LicenseLockGate />
        </main>
        {IS_ANDROID && <BottomTabBar />}
      </div>

      {IS_ANDROID && <NavOverlay open={navOpen} onClose={() => setNavOpen(false)} />}
    </div>
  );
}
