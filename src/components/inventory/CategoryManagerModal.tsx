import { useRef, useState } from "react";
import { ImagePlus, Pencil, Plus, Tags, Trash2 } from "lucide-react";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { ModalCard, inputClass } from "../ui/ModalCard";
import { ConfirmDialog } from "../ui/ConfirmDialog";
import { ItemImage } from "../billing/ItemImage";
import { useInventoryStore } from "../../store";
import { uploadItemImage } from "../../services/inventoryService";
import { categoryIcon } from "../../utils/categoryIcon";
import { readImageFile } from "../../utils/image";
import { runAction } from "../../utils/runAction";
import type { Category } from "../../types";

/** Add, rename, re-photo and delete categories. One editor at the top serves
 * both "new" and "edit"; the list below picks what it edits. */
export function CategoryManagerModal({ onClose }: { onClose: () => void }) {
  const categories = useInventoryStore((state) => state.categories);
  const imageCache = useInventoryStore((state) => state.imageCache);
  const addCategory = useInventoryStore((state) => state.addCategory);
  const updateCategory = useInventoryStore((state) => state.updateCategory);
  const deleteCategory = useInventoryStore((state) => state.deleteCategory);

  const [editing, setEditing] = useState<Category | null>(null);
  const [name, setName] = useState("");
  const [imagePath, setImagePath] = useState<string | null>(null);
  const [isTopping, setIsTopping] = useState(false);
  const [localPreview, setLocalPreview] = useState<string | null>(null);
  const [isUploading, setIsUploading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<Category | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const previewUrl = localPreview ?? (imagePath ? imageCache[imagePath] : undefined);

  const startEditing = (category: Category | null) => {
    setEditing(category);
    setName(category?.name ?? "");
    setImagePath(category?.imagePath ?? null);
    setIsTopping(category?.isTopping ?? false);
    setLocalPreview(null);
    setError(null);
  };

  const pickImage = async (file: File | undefined) => {
    if (!file) return;
    await runAction(setIsUploading, setError, async () => {
      try {
        const { base64, extension } = await readImageFile(file);
        setLocalPreview(`data:image/${extension === "jpg" ? "jpeg" : extension};base64,${base64}`);
        setImagePath(await uploadItemImage(base64, extension));
      } catch (e) {
        setLocalPreview(null);
        throw e;
      } finally {
        if (fileInputRef.current) fileInputRef.current.value = "";
      }
    });
  };

  const save = async () => {
    if (!name.trim()) {
      setError("Enter a category name.");
      return;
    }
    const ok = await runAction(setIsSaving, setError, () =>
      editing ? updateCategory(editing.id, name.trim(), imagePath, isTopping) : addCategory(name.trim(), imagePath, isTopping),
    );
    if (ok) startEditing(null);
  };

  const confirmDelete = async () => {
    if (!deleting) return;
    const ok = await runAction(setIsDeleting, setDeleteError, () => deleteCategory(deleting.id));
    if (!ok) return;
    if (editing?.id === deleting.id) startEditing(null);
    setDeleting(null);
  };

  return (
    <ModalCard title="Categories" subtitle={`${categories.length} categories`} icon={Tags} onClose={onClose}>
      <form
        className="rounded-xl border border-line bg-surface-2/40 p-3"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <p className="mb-2 text-xs font-semibold text-fg-muted">{editing ? `Edit “${editing.name}”` : "New category"}</p>
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => fileInputRef.current?.click()}
            title={previewUrl ? "Change photo" : "Add photo"}
            className="relative flex h-14 w-14 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-dashed border-line-strong bg-surface text-fg-muted hover:border-accent hover:text-accent-text"
          >
            {previewUrl ? <img src={previewUrl} alt="" className="h-full w-full object-cover" /> : <ImagePlus className="h-5 w-5" />}
            {isUploading && <span className="absolute inset-0 flex items-center justify-center bg-surface/70 text-xs">…</span>}
          </button>
          <input
            ref={fileInputRef}
            type="file"
            accept="image/jpeg,image/png,image/webp,image/gif"
            className="hidden"
            onChange={(e) => void pickImage(e.target.files?.[0])}
          />
          <div className="min-w-0 flex-1">
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Category name, e.g. Drinks"
              aria-label="Category name"
              className={inputClass}
              autoFocus
            />
            {previewUrl && (
              <button
                type="button"
                onClick={() => {
                  setImagePath(null);
                  setLocalPreview(null);
                }}
                className="mt-1 text-xs text-fg-muted hover:text-danger-text"
              >
                Remove photo
              </button>
            )}
          </div>
        </div>
        <div className="mt-3 flex items-center justify-between gap-3">
          <div>
            <p className="text-sm font-medium text-fg">Topping group</p>
            <p className="text-xs text-fg-muted">Its items are offered as toppings and hidden from the billing menu.</p>
          </div>
          <ToggleSwitch checked={isTopping} onChange={setIsTopping} label="Topping group" />
        </div>
        {error && <p className="mt-2 text-xs text-danger-text">{error}</p>}
        <div className="mt-3 flex justify-end gap-2">
          {editing && (
            <button type="button" onClick={() => startEditing(null)} className="rounded-xl border border-line px-3 py-2 text-sm font-medium text-fg-2 hover:bg-surface-2">
              Cancel
            </button>
          )}
          <button
            type="submit"
            disabled={isSaving || isUploading}
            className="flex items-center gap-1.5 rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-accent-ink hover:bg-accent-hover disabled:opacity-50"
          >
            {!editing && <Plus className="h-4 w-4" />}
            {isSaving ? "Saving…" : editing ? "Save changes" : "Add category"}
          </button>
        </div>
      </form>

      {categories.length === 0 ? (
        <p className="py-8 text-center text-sm text-fg-muted">No categories yet — add your first one above.</p>
      ) : (
        <ul className="mt-4 divide-y divide-line">
          {categories.map((category) => {
            const Icon = categoryIcon(category.name);
            return (
              <li key={category.id} className={`flex items-center gap-3 py-2.5 ${editing?.id === category.id ? "opacity-60" : ""}`}>
                <ItemImage
                  imagePath={category.imagePath}
                  alt=""
                  className="h-10 w-10 shrink-0 rounded-lg"
                  fallback={<Icon className="h-5 w-5" />}
                />
                <span className="min-w-0 flex-1 truncate text-sm font-medium text-fg">{category.name}</span>
                {category.isTopping && (
                  <span className="shrink-0 rounded-full bg-accent-soft px-2 py-0.5 text-[11px] font-medium text-accent-text">Toppings</span>
                )}
                <button
                  type="button"
                  onClick={() => startEditing(category)}
                  aria-label={`Edit ${category.name}`}
                  className="rounded-lg p-2 text-fg-muted hover:bg-surface-2 hover:text-fg-2"
                >
                  <Pencil className="h-4 w-4" />
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setDeleteError(null);
                    setDeleting(category);
                  }}
                  aria-label={`Delete ${category.name}`}
                  className="rounded-lg p-2 text-fg-muted hover:bg-danger-soft hover:text-danger-text"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </li>
            );
          })}
        </ul>
      )}

      {deleting && (
        <ConfirmDialog
          title={`Delete “${deleting.name}”?`}
          message={deleteError ?? "Its products stay in inventory, just without a category."}
          confirmLabel="Delete"
          isDangerous
          isBusy={isDeleting}
          onConfirm={() => void confirmDelete()}
          onCancel={() => setDeleting(null)}
        />
      )}
    </ModalCard>
  );
}
