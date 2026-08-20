import { useEffect, useState } from "react";
import { getLogoDataUrl } from "../services/configService";

/**
 * Resolves `app_config.logoPath` (a stored filename) to a renderable
 * `data:` URL, refetching whenever it changes — shared by every place that
 * displays the logo (TopBar, Settings' preview) so a fresh upload appears
 * everywhere it's shown without a restart, and so this fetch-on-change
 * logic exists in exactly one place.
 */
export function useLogoDataUrl(logoPath: string | null): string | null {
  const [dataUrl, setDataUrl] = useState<string | null>(null);

  useEffect(() => {
    if (!logoPath) {
      setDataUrl(null);
      return;
    }
    let cancelled = false;
    getLogoDataUrl(logoPath)
      .then((url) => {
        if (!cancelled) setDataUrl(url);
      })
      .catch(() => {
        if (!cancelled) setDataUrl(null);
      });
    return () => {
      cancelled = true;
    };
  }, [logoPath]);

  return dataUrl;
}
