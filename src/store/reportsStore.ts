import { create } from "zustand";
import { getSalesOverTime, getSalesSummary, getTopItems } from "../services/reportsService";
import { resolveDateRangePreset } from "../utils/dateRanges";
import type { DailySales, DateRangePreset, SalesSummary, TopItem, TopItemSort } from "../types";

const TOP_ITEMS_LIMIT = 10;

interface ReportsState {
  preset: DateRangePreset;
  startDate: string;
  endDate: string;
  topItemsSort: TopItemSort;

  summary: SalesSummary | null;
  topItems: TopItem[];
  series: DailySales[];
  isLoading: boolean;
  error: string | null;

  setPreset: (preset: Exclude<DateRangePreset, "custom">) => void;
  setCustomRange: (startDate: string, endDate: string) => void;
  setTopItemsSort: (sort: TopItemSort) => void;
  load: () => Promise<void>;
}

const initialRange = resolveDateRangePreset("today");

export const useReportsStore = create<ReportsState>((set, get) => ({
  preset: "today",
  startDate: initialRange.startDate,
  endDate: initialRange.endDate,
  topItemsSort: "quantity",

  summary: null,
  topItems: [],
  series: [],
  isLoading: false,
  error: null,

  setPreset: (preset) => {
    const range = resolveDateRangePreset(preset);
    set({ preset, ...range });
    void get().load();
  },

  setCustomRange: (startDate, endDate) => {
    set({ preset: "custom", startDate, endDate });
    void get().load();
  },

  setTopItemsSort: (topItemsSort) => {
    set({ topItemsSort });
    void get().load();
  },

  load: async () => {
    const { startDate, endDate, topItemsSort } = get();
    set({ isLoading: true, error: null });
    try {
      const [summary, topItems, series] = await Promise.all([
        getSalesSummary(startDate, endDate),
        getTopItems(startDate, endDate, TOP_ITEMS_LIMIT, topItemsSort),
        getSalesOverTime(startDate, endDate),
      ]);
      set({ summary, topItems, series, isLoading: false });
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
    }
  },
}));
