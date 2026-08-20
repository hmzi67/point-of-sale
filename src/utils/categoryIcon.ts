import {
  Candy,
  CakeSlice,
  Carrot,
  Coffee,
  Cookie,
  Croissant,
  Donut,
  Drumstick,
  Fish,
  IceCreamCone,
  Milk,
  Pill,
  Pizza,
  Sandwich,
  Shirt,
  ShoppingBasket,
  Soup,
  UtensilsCrossed,
  Wheat,
  Wrench,
  type LucideIcon,
} from "lucide-react";

/**
 * A line-icon per category, picked by keyword match against the category
 * name rather than a fixed name→icon map — categories are entirely
 * client-defined (Inventory → Add category, see `categoryColor.ts` for the
 * same reasoning applied to color), so a bakery's "Bread" and a pharmacy's
 * "Medicine" both need to resolve to *something* sensible without either
 * one being hardcoded as the only supported case. First keyword match wins;
 * order matters (more specific terms before their broader siblings, e.g.
 * "grocery" before a hypothetical generic "store").
 */
const KEYWORD_ICONS: { keywords: string[]; icon: LucideIcon }[] = [
  { keywords: ["bread", "loaf", "bakery"], icon: Wheat },
  { keywords: ["cake"], icon: CakeSlice },
  { keywords: ["donut", "doughnut"], icon: Donut },
  { keywords: ["pastry", "croissant"], icon: Croissant },
  { keywords: ["sandwich", "sub", "burger"], icon: Sandwich },
  { keywords: ["cookie", "biscuit", "snack"], icon: Cookie },
  { keywords: ["candy", "sweet"], icon: Candy },
  { keywords: ["ice cream", "icecream", "gelato"], icon: IceCreamCone },
  { keywords: ["coffee", "tea", "drink", "beverage"], icon: Coffee },
  { keywords: ["milk", "dairy"], icon: Milk },
  { keywords: ["fruit", "produce", "vegetable", "veg"], icon: Carrot },
  { keywords: ["meat", "chicken"], icon: Drumstick },
  { keywords: ["fish", "seafood"], icon: Fish },
  { keywords: ["pizza"], icon: Pizza },
  { keywords: ["soup"], icon: Soup },
  { keywords: ["grocery"], icon: ShoppingBasket },
  { keywords: ["medicine", "pharmacy", "drug", "pill", "health"], icon: Pill },
  { keywords: ["hardware", "tool"], icon: Wrench },
  { keywords: ["clothing", "apparel", "garment", "fashion"], icon: Shirt },
];

/** Unmatched categories (any client's own naming this list hasn't seen) get
 * a neutral "menu item" glyph rather than nothing. */
const DEFAULT_ICON: LucideIcon = UtensilsCrossed;

export function categoryIcon(categoryName: string): LucideIcon {
  const lower = categoryName.toLowerCase();
  const match = KEYWORD_ICONS.find(({ keywords }) => keywords.some((keyword) => lower.includes(keyword)));
  return match?.icon ?? DEFAULT_ICON;
}
