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
import { getCounters } from "../services/countersService";
import type { Category, Counter, DeleteOutcome, Item, ItemInput } from "../types";

interface InventoryState {
  items: Item[];
  categories: Category[];
  /** Active counters only — the item-form dropdown's source list. Loaded
   * alongside items/categories; a management screen that needs inactive ones
   * too calls `getCounters(true)` directly rather than through this store. */
  counters: Counter[];
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
  /** Deletes/archives each id in turn (same per-item rules as `deleteItem`,
   * one `inventory_delete_item` call per id — there's no bulk backend
   * command), reloading the list once at the end rather than after every
   * item. A single failure doesn't stop the rest; the tally plus any error
   * messages are returned so the caller can summarize what happened. */
  deleteItems: (ids: number[]) => Promise<{ deleted: number; archived: number; failed: { id: number; error: string }[] }>;
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
  counters: [],
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
      const [items, categories, counters] = await Promise.all([
        getItems({ search: search || undefined, categoryId: categoryId ?? undefined }),
        getCategories(),
        getCounters(),
      ]);
      set({ items, categories, counters, isLoaded: true, isLoading: false });
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

  deleteItems: async (ids) => {
    let deleted = 0;
    let archived = 0;
    const failed: { id: number; error: string }[] = [];
    for (const id of ids) {
      try {
        const outcome = await deleteItemCommand(id);
        if (outcome === "archived") archived += 1;
        else deleted += 1;
      } catch (error) {
        failed.push({ id, error: (error as Error).message });
      }
    }
    await get().load();
    return { deleted, archived, failed };
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
