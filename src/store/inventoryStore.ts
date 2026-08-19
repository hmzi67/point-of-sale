import { create } from "zustand";
import {
  addCategory as addCategoryCommand,
  addItem as addItemCommand,
  deleteItem as deleteItemCommand,
  getCategories,
  getItemImage,
  getItems,
  updateItem as updateItemCommand,
} from "../services/inventoryService";
import type { Category, DeleteOutcome, Item, ItemInput } from "../types";

interface InventoryState {
  items: Item[];
  categories: Category[];
  isLoaded: boolean;
  isLoading: boolean;
  error: string | null;

  search: string;
  categoryId: number | null;
  setSearch: (search: string) => void;
  setCategoryId: (categoryId: number | null) => void;

  load: () => Promise<void>;
  addItem: (input: ItemInput) => Promise<Item>;
  updateItem: (id: number, input: ItemInput) => Promise<Item>;
  deleteItem: (id: number) => Promise<DeleteOutcome>;
  addCategory: (name: string) => Promise<Category>;

  /** filename -> data URL, shared so a table of thumbnails fetches each
   * image once no matter how many rows/rerenders reference it. */
  imageCache: Record<string, string>;
  /** Fetches and caches an image if not already cached; no-op otherwise. */
  ensureImage: (fileName: string) => void;
}

/**
 * List state plus CRUD, backed by the `inventory_*` commands. Filtering
 * (search + category) is re-queried from SQLite rather than done client-side,
 * so behaviour matches what a later barcode-scan-to-cart lookup in Billing
 * will see.
 */
export const useInventoryStore = create<InventoryState>((set, get) => ({
  items: [],
  categories: [],
  isLoaded: false,
  isLoading: false,
  error: null,

  search: "",
  categoryId: null,
  setSearch: (search) => {
    set({ search });
    void get().load();
  },
  setCategoryId: (categoryId) => {
    set({ categoryId });
    void get().load();
  },

  load: async () => {
    set({ isLoading: true, error: null });
    try {
      const { search, categoryId } = get();
      const [items, categories] = await Promise.all([
        getItems({ search: search || undefined, categoryId: categoryId ?? undefined }),
        getCategories(),
      ]);
      set({ items, categories, isLoaded: true, isLoading: false });
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false, isLoaded: true });
    }
  },

  addItem: async (input) => {
    const item = await addItemCommand(input);
    await get().load();
    return item;
  },

  updateItem: async (id, input) => {
    const item = await updateItemCommand(id, input);
    await get().load();
    return item;
  },

  deleteItem: async (id) => {
    const outcome = await deleteItemCommand(id);
    await get().load();
    return outcome;
  },

  addCategory: async (name) => {
    const category = await addCategoryCommand(name);
    set((state) => ({ categories: [...state.categories, category].sort((a, b) => a.name.localeCompare(b.name)) }));
    return category;
  },

  imageCache: {},
  ensureImage: (fileName) => {
    if (get().imageCache[fileName]) return;
    void getItemImage(fileName)
      .then((dataUrl) => {
        set((state) => ({ imageCache: { ...state.imageCache, [fileName]: dataUrl } }));
      })
      .catch(() => {
        // A missing/corrupt image file shouldn't break the list — the row
        // just falls back to its placeholder icon.
      });
  },
}));
