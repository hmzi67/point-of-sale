import { useEffect, useRef, useState } from "react";
import { ImagePlus, Plus, X } from "lucide-react";
import { useInventoryStore } from "../../store";
import { uploadItemImage } from "../../services/inventoryService";
import { decimalToMinor, minorToDecimal } from "../../utils/format";
import { readImageFile } from "../../utils/image";
import type { Item, ItemInput } from "../../types";

interface ItemFormModalProps {
  /** Omit to add a new item; pass an item to edit it. */
  item?: Item;
  onClose: () => void;
}

interface FormState {
  name: string;
  barcode: string;
  /** Short blurb shown on the billing screen's item detail modal. */
  description: string;
  price: string;
  cost: string;
  stockQty: string;
  categoryId: number | null;
  lowStockThreshold: string;
  /** Filename already saved on disk (via `inventory_upload_image`), or `null`
   * if this item has no photo. Only ever set once an upload has completed —
   * never a pending/local-only value. */
  imagePath: string | null;
  /** "Sold by amount" — a loose/weighed item (channa, rice, dry fruits) a
   * cashier sells by typing a rupee amount rather than a quantity. */
  soldByAmount: boolean;
  /** Display unit for this item's quantity (e.g. "kg") — only meaningful
   * when `soldByAmount` is set. */
  unit: string;
}

function toFormState(item?: Item): FormState {
  return {
    name: item?.name ?? "",
    barcode: item?.barcode ?? "",
    description: item?.description ?? "",
    price: item ? String(minorToDecimal(item.priceMinor)) : "",
    cost: item ? String(minorToDecimal(item.costMinor)) : "",
    stockQty: item ? String(item.stockQty) : "0",
    categoryId: item?.categoryId ?? null,
    lowStockThreshold: item ? String(item.lowStockThreshold) : "5",
    imagePath: item?.imagePath ?? null,
    soldByAmount: item?.soldByAmount ?? false,
    unit: item?.unit ?? "",
  };
}

/** Add/edit form for one item, including inline "add a new category" and a
 * product photo. */
export function ItemFormModal({ item, onClose }: ItemFormModalProps) {
  const categories = useInventoryStore((state) => state.categories);
  const addItem = useInventoryStore((state) => state.addItem);
  const updateItem = useInventoryStore((state) => state.updateItem);
  const addCategory = useInventoryStore((state) => state.addCategory);
  const imageCache = useInventoryStore((state) => state.imageCache);
  const ensureImage = useInventoryStore((state) => state.ensureImage);

  const [form, setForm] = useState<FormState>(() => toFormState(item));
  const [isAddingCategory, setIsAddingCategory] = useState(false);
  const [newCategoryName, setNewCategoryName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  // The image the form currently shows — a freshly read local file's data URL
  // takes priority over whatever is cached for the item's stored filename, so
  // a just-picked photo previews instantly without waiting on a re-fetch.
  const [localPreview, setLocalPreview] = useState<string | null>(null);
  const [isUploadingImage, setIsUploadingImage] = useState(false);
  const [imageError, setImageError] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const isEditing = Boolean(item);
  const previewUrl = localPreview ?? (form.imagePath ? imageCache[form.imagePath] : undefined);

  useEffect(() => {
    if (item?.imagePath) ensureImage(item.imagePath);
  }, [item?.imagePath, ensureImage]);

  useEffect(() => {
    // Barcode scanners send Enter after typing the code; outside a text
    // field that would otherwise submit or bubble unexpectedly.
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const set = <K extends keyof FormState>(key: K, value: FormState[K]) =>
    setForm((current) => ({ ...current, [key]: value }));

  const submitNewCategory = async () => {
    if (!newCategoryName.trim()) return;
    try {
      const category = await addCategory(newCategoryName.trim());
      set("categoryId", category.id);
      setNewCategoryName("");
      setIsAddingCategory(false);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const handleImageSelected = async (file: File | undefined) => {
    if (!file) return;
    setImageError(null);

    try {
      const { base64, extension } = await readImageFile(file);
      // Preview immediately from the bytes already in hand — no need to wait
      // for the upload (or a later re-fetch) just to show what was picked.
      setLocalPreview(`data:image/${extension === "jpg" ? "jpeg" : extension};base64,${base64}`);

      setIsUploadingImage(true);
      const fileName = await uploadItemImage(base64, extension);
      set("imagePath", fileName);
    } catch (e) {
      setImageError((e as Error).message);
      setLocalPreview(null);
    } finally {
      setIsUploadingImage(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  };

  const handleRemoveImage = () => {
    set("imagePath", null);
    setLocalPreview(null);
    setImageError(null);
  };

  const submit = async () => {
    setError(null);

    const price = Number(form.price);
    const cost = Number(form.cost || "0");
    const stockQty = Number(form.stockQty);
    const lowStockThreshold = Number(form.lowStockThreshold);

    if (!form.name.trim()) return setError("Item name is required.");
    if (!Number.isFinite(price) || price < 0) return setError("Enter a valid price.");
    if (!Number.isFinite(cost) || cost < 0) return setError("Enter a valid cost.");
    // A "sold by amount" item's stock is a fractional real-world quantity
    // (kg, ltr) — only a normal per-piece item is required to be whole.
    if (!Number.isFinite(stockQty) || stockQty < 0) return setError("Enter a valid stock quantity.");
    if (!form.soldByAmount && !Number.isInteger(stockQty)) {
      return setError("Stock quantity must be a whole number (or turn on \"Sold by amount\" for a weighed item).");
    }
    if (!Number.isInteger(lowStockThreshold) || lowStockThreshold < 0) {
      return setError("Low-stock threshold must be a whole number.");
    }

    const input: ItemInput = {
      name: form.name.trim(),
      barcode: form.barcode.trim() || null,
      description: form.description.trim() || null,
      priceMinor: decimalToMinor(price),
      costMinor: decimalToMinor(cost),
      stockQty,
      categoryId: form.categoryId,
      lowStockThreshold,
      imagePath: form.imagePath,
      soldByAmount: form.soldByAmount,
      unit: form.unit.trim() || null,
    };

    setIsSaving(true);
    try {
      if (item) {
        await updateItem(item.id, input);
      } else {
        await addItem(input);
      }
      onClose();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="w-full max-w-md rounded-lg bg-white shadow-xl">
        <div className="flex items-center justify-between border-b border-slate-200 px-5 py-4">
          <h3 className="text-base font-semibold text-slate-900">
            {isEditing ? "Edit item" : "Add item"}
          </h3>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600"
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <form
          className="max-h-[75dvh] space-y-4 overflow-y-auto px-5 py-4"
          onSubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

          <div>
            <span className="block text-xs font-medium text-slate-500">Product image</span>
            <div className="mt-1 flex items-center gap-3">
              <button
                type="button"
                onClick={() => fileInputRef.current?.click()}
                className="relative flex h-16 w-16 shrink-0 items-center justify-center overflow-hidden rounded-md border border-dashed border-slate-300 bg-slate-50 text-slate-400 hover:border-brand-400 hover:text-brand-500"
              >
                {previewUrl ? (
                  <img src={previewUrl} alt="" className="h-full w-full object-cover" />
                ) : (
                  <ImagePlus className="h-5 w-5" />
                )}
                {isUploadingImage && (
                  <span className="absolute inset-0 flex items-center justify-center bg-white/70 text-[10px] font-medium text-slate-600">
                    …
                  </span>
                )}
              </button>

              <div className="flex flex-col gap-1">
                <button
                  type="button"
                  onClick={() => fileInputRef.current?.click()}
                  className="text-left text-sm font-medium text-brand-600 hover:text-brand-700"
                >
                  {previewUrl ? "Change photo" : "Upload photo"}
                </button>
                {previewUrl && (
                  <button
                    type="button"
                    onClick={handleRemoveImage}
                    className="text-left text-xs text-slate-500 hover:text-red-600"
                  >
                    Remove photo
                  </button>
                )}
                <span className="text-xs text-slate-400">JPG, PNG, WEBP or GIF, up to 5 MB</span>
              </div>

              <input
                ref={fileInputRef}
                type="file"
                accept="image/jpeg,image/png,image/webp,image/gif"
                className="hidden"
                onChange={(e) => void handleImageSelected(e.target.files?.[0])}
              />
            </div>
            {imageError && <p className="mt-1.5 text-xs text-red-600">{imageError}</p>}
          </div>

          <div>
            <label className="block text-xs font-medium text-slate-500" htmlFor="item-name">
              Name
            </label>
            <input
              id="item-name"
              value={form.name}
              onChange={(e) => set("name", e.target.value)}
              className="mt-1 w-full rounded-md border border-slate-300 px-3 py-2 text-sm"
              autoFocus
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-slate-500" htmlFor="item-barcode">
              SKU / Barcode
            </label>
            {/* A plain text input: scanners type the code and send Enter — no
                special hardware integration needed. */}
            <input
              id="item-barcode"
              value={form.barcode}
              onChange={(e) => set("barcode", e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") e.preventDefault();
              }}
              placeholder="Scan or type…"
              className="mt-1 w-full rounded-md border border-slate-300 px-3 py-2 text-sm"
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-slate-500" htmlFor="item-description">
              Description <span className="text-slate-400">(optional)</span>
            </label>
            <textarea
              id="item-description"
              value={form.description}
              onChange={(e) => set("description", e.target.value)}
              rows={2}
              placeholder="A short blurb shown when a cashier taps this item on the billing screen…"
              className="mt-1 w-full resize-none rounded-md border border-slate-300 px-3 py-2 text-sm"
            />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-slate-500" htmlFor="item-price">
                Price
              </label>
              <input
                id="item-price"
                type="number"
                min="0"
                step="0.01"
                inputMode="decimal"
                value={form.price}
                onChange={(e) => set("price", e.target.value)}
                className="mt-1 w-full rounded-md border border-slate-300 px-3 py-2 text-sm"
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-slate-500" htmlFor="item-cost">
                Cost
              </label>
              <input
                id="item-cost"
                type="number"
                min="0"
                step="0.01"
                inputMode="decimal"
                value={form.cost}
                onChange={(e) => set("cost", e.target.value)}
                className="mt-1 w-full rounded-md border border-slate-300 px-3 py-2 text-sm"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-slate-500" htmlFor="item-stock">
                Stock qty
              </label>
              <input
                id="item-stock"
                type="number"
                min="0"
                step={form.soldByAmount ? "0.01" : "1"}
                inputMode="decimal"
                value={form.stockQty}
                onChange={(e) => set("stockQty", e.target.value)}
                className="mt-1 w-full rounded-md border border-slate-300 px-3 py-2 text-sm"
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-slate-500" htmlFor="item-threshold">
                Low-stock at
              </label>
              <input
                id="item-threshold"
                type="number"
                min="0"
                step="1"
                inputMode="numeric"
                value={form.lowStockThreshold}
                onChange={(e) => set("lowStockThreshold", e.target.value)}
                className="mt-1 w-full rounded-md border border-slate-300 px-3 py-2 text-sm"
              />
            </div>
          </div>

          <div className="rounded-md border border-slate-200 p-3">
            <label className="flex items-center gap-2 text-sm font-medium text-slate-700">
              <input
                type="checkbox"
                checked={form.soldByAmount}
                onChange={(e) => set("soldByAmount", e.target.checked)}
                className="h-4 w-4 rounded border-slate-300 text-brand-600 focus:ring-brand-400"
              />
              Sold by amount
            </label>
            <p className="mt-1 text-xs text-slate-500">
              For loose/weighed items (channa, rice, dry fruits) — on the billing screen, a cashier can type a rupee
              amount ("100 rupees worth") instead of a quantity, and the app works out how much that buys from this
              item's price.
            </p>
            {form.soldByAmount && (
              <div className="mt-2">
                <label className="block text-xs font-medium text-slate-500" htmlFor="item-unit">
                  Unit
                </label>
                <input
                  id="item-unit"
                  value={form.unit}
                  onChange={(e) => set("unit", e.target.value)}
                  placeholder="e.g. kg"
                  className="mt-1 w-full max-w-[8rem] rounded-md border border-slate-300 px-3 py-2 text-sm"
                />
                <p className="mt-1 text-[11px] text-slate-400">
                  Shown on the cart line and receipt, e.g. "0.77 kg". Optional.
                </p>
              </div>
            )}
          </div>

          <div>
            <label className="block text-xs font-medium text-slate-500" htmlFor="item-category">
              Category
            </label>

            {isAddingCategory ? (
              <div className="mt-1 flex gap-2">
                <input
                  autoFocus
                  value={newCategoryName}
                  onChange={(e) => setNewCategoryName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      void submitNewCategory();
                    }
                  }}
                  placeholder="New category name"
                  className="w-full rounded-md border border-slate-300 px-3 py-2 text-sm"
                />
                <button
                  type="button"
                  onClick={() => void submitNewCategory()}
                  className="rounded-md bg-brand-600 px-3 py-2 text-sm font-medium text-white hover:bg-brand-700"
                >
                  Add
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setIsAddingCategory(false);
                    setNewCategoryName("");
                  }}
                  className="rounded-md px-3 py-2 text-sm text-slate-500 hover:bg-slate-100"
                >
                  Cancel
                </button>
              </div>
            ) : (
              <div className="mt-1 flex gap-2">
                <select
                  id="item-category"
                  value={form.categoryId ?? ""}
                  onChange={(e) => set("categoryId", e.target.value ? Number(e.target.value) : null)}
                  className="w-full rounded-md border border-slate-300 px-3 py-2 text-sm"
                >
                  <option value="">Uncategorized</option>
                  {categories.map((category) => (
                    <option key={category.id} value={category.id}>
                      {category.name}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  onClick={() => setIsAddingCategory(true)}
                  className="flex shrink-0 items-center gap-1 rounded-md border border-slate-300 px-3 py-2 text-sm text-slate-600 hover:bg-slate-50"
                >
                  <Plus className="h-4 w-4" />
                  New
                </button>
              </div>
            )}
          </div>

          <div className="flex justify-end gap-2 border-t border-slate-200 pt-4">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md px-3 py-1.5 text-sm font-medium text-slate-600 hover:bg-slate-100"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSaving || isUploadingImage}
              className="rounded-md bg-brand-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-50"
            >
              {isSaving ? "Saving…" : isUploadingImage ? "Uploading photo…" : isEditing ? "Save changes" : "Add item"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
