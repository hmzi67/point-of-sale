import { useEffect } from "react";
import { Package } from "lucide-react";
import { useInventoryStore } from "../../store";

interface ItemThumbnailProps {
  imagePath: string | null;
  name: string;
  size?: number;
}

/** A small product photo, or a placeholder icon when none is set. Fetches
 * lazily through the shared store cache so re-renders never re-fetch. */
export function ItemThumbnail({ imagePath, name, size = 36 }: ItemThumbnailProps) {
  const dataUrl = useInventoryStore((state) => (imagePath ? state.imageCache[imagePath] : undefined));
  const ensureImage = useInventoryStore((state) => state.ensureImage);

  useEffect(() => {
    if (imagePath) ensureImage(imagePath);
  }, [imagePath, ensureImage]);

  const style = { width: size, height: size };

  if (!imagePath) {
    return (
      <span
        style={style}
        className="flex shrink-0 items-center justify-center rounded-xl border border-accent/10 bg-accent-soft text-accent-text"
      >
        <Package className="h-5 w-5" strokeWidth={1.5} />
      </span>
    );
  }

  if (!dataUrl) {
    return <span style={style} className="shrink-0 animate-pulse rounded-xl bg-surface-3" />;
  }

  return (
    <img
      src={dataUrl}
      alt={name}
      style={style}
      className="shrink-0 rounded-xl border border-line object-cover"
    />
  );
}
