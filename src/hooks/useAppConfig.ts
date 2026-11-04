import { useAppStore } from "../store";

/** Read-only access to the installation config loaded at startup. */
export function useAppConfig() {
  const config = useAppStore((state) => state.config);
  const isLoading = useAppStore((state) => state.isLoadingConfig);
  return { config, isLoading };
}

/** Whether this business runs restaurant workflows (holding orders, kitchen
 * tokens, service charge). Anything else bills directly. Tables and Delivery
 * follow it through their module flags; Rust enforces both. */
export function useIsRestaurant(): boolean {
  return useAppStore((state) => state.config.businessType === "restaurant");
}
