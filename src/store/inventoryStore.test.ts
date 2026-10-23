import { beforeEach, describe, expect, it, vi } from "vitest";
import * as counters from "../services/countersService";
import * as service from "../services/inventoryService";
import type { Category } from "../types";
import { useInventoryStore } from "./inventoryStore";

vi.mock("../services/inventoryService", () => ({
  addCategory: vi.fn(),
  addItem: vi.fn(),
  deleteItem: vi.fn(),
  getCategories: vi.fn(),
  getItemImage: vi.fn(),
  getItems: vi.fn(),
  updateItem: vi.fn(),
}));
vi.mock("../services/countersService", () => ({ getCounters: vi.fn() }));

const mocked = vi.mocked(service);
const store = () => useInventoryStore.getState();

beforeEach(() => {
  vi.resetAllMocks();
  mocked.getItems.mockResolvedValue([]);
  mocked.getCategories.mockResolvedValue([]);
  vi.mocked(counters.getCounters).mockResolvedValue([]);
  useInventoryStore.setState({ search: "", categoryId: null, categories: [], imageCache: {} });
});

describe("inventoryStore", () => {
  it("passes the search and category filters through, omitting empty ones", async () => {
    useInventoryStore.setState({ search: "", categoryId: 3 });
    await store().load();
    expect(mocked.getItems).toHaveBeenCalledWith({ search: undefined, categoryId: 3 });
  });

  it("bulk delete tallies deleted, archived and failed items, then reloads", async () => {
    mocked.deleteItem
      .mockResolvedValueOnce("deleted")
      .mockResolvedValueOnce("archived")
      .mockRejectedValueOnce(new Error("in use"));
    const result = await store().deleteItems([1, 2, 3]);
    expect(result).toEqual({ deleted: 1, archived: 1, failed: [{ id: 3, error: "in use" }] });
    expect(mocked.getItems).toHaveBeenCalledTimes(1);
  });

  it("keeps categories sorted when one is added", async () => {
    useInventoryStore.setState({ categories: [{ id: 1, name: "Snacks" }] as Category[] });
    mocked.addCategory.mockResolvedValue({ id: 2, name: "Beverages" } as Category);
    await store().addCategory("Beverages");
    expect(store().categories.map((c) => c.name)).toEqual(["Beverages", "Snacks"]);
  });

  it("fetches each product image once", async () => {
    mocked.getItemImage.mockResolvedValue("data:image/png;base64,AA");
    store().ensureImage("a.png");
    await vi.waitFor(() => expect(store().imageCache["a.png"]).toBeDefined());
    store().ensureImage("a.png");
    expect(mocked.getItemImage).toHaveBeenCalledTimes(1);
  });
});
