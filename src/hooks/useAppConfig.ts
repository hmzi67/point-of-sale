import { useAppStore } from "../store";

/** Read-only access to the installation config loaded at startup. */
export function useAppConfig() {
  const config = useAppStore((state) => state.config);
  const isLoading = useAppStore((state) => state.isLoadingConfig);
  return { config, isLoading };
}
