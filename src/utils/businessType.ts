import type { BusinessType } from "../types";

export const BUSINESS_TYPES: { value: BusinessType; label: string; hint: string }[] = [
  { value: "retail", label: "Retail shop", hint: "Direct billing: scan, take payment, done. Tables, held orders and kitchen tokens stay hidden." },
  { value: "restaurant", label: "Restaurant / café", hint: "Dine-in tables, held orders and kitchen tokens. Delivery can be switched on under Modules." },
  { value: "other", label: "Other", hint: "Direct billing, the same as a retail shop." },
];
