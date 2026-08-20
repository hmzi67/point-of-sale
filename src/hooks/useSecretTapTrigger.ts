import { useRef } from "react";

const DEFAULT_TAPS = 7;
const DEFAULT_WINDOW_MS = 3000;

/**
 * Returns an `onClick` handler that fires `onTrigger` after `taps` clicks
 * land within `windowMs` of each other — the hidden entry point to the
 * product-owner account (see `components/productOwner`). Attach it to an
 * element that already exists and already looks completely ordinary (the
 * login screen's store icon, the top bar's business name) — this hook adds
 * no visual affordance of its own, on purpose: nothing here should hint
 * that clicking it repeatedly does anything.
 *
 * A slow tap resets the count rather than firing early or erroring, so an
 * ordinary user who happens to double-click the element a couple of times
 * never trips it by accident.
 */
export function useSecretTapTrigger(
  onTrigger: () => void,
  taps: number = DEFAULT_TAPS,
  windowMs: number = DEFAULT_WINDOW_MS,
): () => void {
  const countRef = useRef(0);
  const lastTapRef = useRef(0);

  return () => {
    const now = Date.now();
    if (now - lastTapRef.current > windowMs) {
      countRef.current = 0;
    }
    lastTapRef.current = now;
    countRef.current += 1;

    if (countRef.current >= taps) {
      countRef.current = 0;
      onTrigger();
    }
  };
}
