import type { ReactNode } from "react";

/** The full-screen scrim every modal dialog sits on, centring its card. One
 * place for the stacking order and scrim, so dialogs can't drift apart. */
export function ModalOverlay({ children }: { children: ReactNode }) {
  return <div className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-4">{children}</div>;
}
