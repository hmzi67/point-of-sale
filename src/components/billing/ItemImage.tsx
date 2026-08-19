import { useEffect } from "react";
import { ImageOff } from "lucide-react";
import { useInventoryStore } from "../../store";

interface ItemImageProps {
  imagePath: string | null;
  alt: string;
  className?: string;
}

/**
 * Product photo, or a plain placeholder — never a fake stock photo. Reuses
 * Inventory's existing `imageCache`/`ensureImage` (Phase 3), so a thumbnail
 * already fetched for the Inventory screen (or another item card) is never
 * re-fetched here, and vice versa.
 */
export function ItemImage({ imagePath, alt, className = "" }: ItemImageProps) {
  const dataUrl = useInventoryStore((state) => (imagePath ? state.imageCache[imagePath] : undefined));
  const ensureImage = useInventoryStore((state) => state.ensureImage);

  useEffect(() => {
    if (imagePath) ensureImage(imagePath);
  }, [imagePath, ensureImage]);

  if (imagePath && dataUrl) {
    return <img src={dataUrl} alt={alt} className={`object-cover ${className}`} />;
  }

  return (
    <div className={`flex items-center justify-center bg-gradient-to-br from-slate-100 to-slate-200 text-slate-400 ${className}`}>
      <ImageOff className="h-1/3 w-1/3 min-h-4 min-w-4" />
    </div>
  );
}
