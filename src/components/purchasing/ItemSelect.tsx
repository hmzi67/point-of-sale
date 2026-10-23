import type { Item } from "../../types";

/** A `<select>` of catalogue items by name — for purchase and receipt lines. */
export function ItemSelect({
  items,
  value,
  onChange,
}: {
  items: Item[];
  value: number | null;
  onChange: (item: Item) => void;
}) {
  return (
    <select
      value={value ?? ""}
      onChange={(e) => {
        const item = items.find((i) => i.id === Number(e.target.value));
        if (item) onChange(item);
      }}
      aria-label="Item"
      className="w-full min-w-40 rounded-lg border border-line-strong bg-surface py-2 pl-2 pr-8 text-sm"
    >
      <option value="" disabled>
        Choose item…
      </option>
      {items.map((item) => (
        <option key={item.id} value={item.id}>
          {item.name}
          {item.barcode ? ` (${item.barcode})` : ""}
        </option>
      ))}
    </select>
  );
}
