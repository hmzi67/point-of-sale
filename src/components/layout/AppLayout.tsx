import { useState } from "react";
import { Outlet } from "react-router-dom";
import { NavOverlay } from "./NavOverlay";
import { TopBar } from "./TopBar";

/** The hamburger-triggered `NavOverlay` replaced the old always-visible dark
 * sidebar app-wide — this is the one place its open/closed state lives, so
 * the hamburger button (in `TopBar`) and the overlay itself can be siblings
 * without prop-drilling through every page. */
export function AppLayout() {
  const [navOpen, setNavOpen] = useState(false);

  return (
    <div className="flex h-full bg-canvas">
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar onOpenNav={() => setNavOpen(true)} />
        <main className="flex-1 overflow-y-auto p-4 sm:p-6">
          <Outlet />
        </main>
      </div>

      <NavOverlay open={navOpen} onClose={() => setNavOpen(false)} />
    </div>
  );
}
