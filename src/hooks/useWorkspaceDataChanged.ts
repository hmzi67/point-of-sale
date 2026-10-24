import { useEffect, useRef } from "react";
import { WORKSPACE_DATA_CHANGED } from "../store/workspaceStore";

/**
 * Calls `onChange` when records from another counter of the business have
 * just been applied here (see `workspaceStore`), so a screen holding its own
 * copy of shared data can reload it. The latest `onChange` is always used.
 */
export function useWorkspaceDataChanged(onChange: () => void) {
  const latest = useRef(onChange);
  latest.current = onChange;
  useEffect(() => {
    const handler = () => latest.current();
    window.addEventListener(WORKSPACE_DATA_CHANGED, handler);
    return () => window.removeEventListener(WORKSPACE_DATA_CHANGED, handler);
  }, []);
}
