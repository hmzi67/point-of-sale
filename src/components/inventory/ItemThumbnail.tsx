import { useEffect } from "react";
import { ImageOff } from "lucide-react";
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
        className="flex shrink-0 items-center justify-center rounded-md bg-slate-100 text-slate-300"
      >
        <ImageOff className="h-4 w-4" />
      </span>
    );
  }

  if (!dataUrl) {
    return <span style={style} className="shrink-0 animate-pulse rounded-md bg-slate-100" />;
  }

  return (
    <img
      src={dataUrl}
      alt={name}
      style={style}
      className="shrink-0 rounded-md border border-slate-200 object-cover"
    />
  );
}
