import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";

export interface DropdownOption<T extends string | number> {
  value: T;
  label: string;
  disabled?: boolean;
  /** Optional right-aligned secondary content per option — e.g. the table
   * selector's status dot + "Free"/"Occupied" text. */
  meta?: React.ReactNode;
}

interface DropdownSelectProps<T extends string | number> {
  value: T | null;
  placeholder: string;
  options: DropdownOption<T>[];
  onChange: (value: T) => void;
  disabled?: boolean;
}

/**
 * A card-style replacement for a native `<select>`: white floating panel,
 * rounded corners, soft shadow, dividers between options, hover tint, and an
 * accent checkmark on the selected row. Native `<select>` popups are styled
 * by the OS/browser, not CSS, which is why the old dark flat menu couldn't be
 * restyled in place — this renders the option list itself instead.
 *
 * Shared by both the Table and Order Type dropdowns so they stay visually
 * identical; behavior (select → close → call `onChange`) matches the native
 * select it replaced.
 */
export function DropdownSelect<T extends string | number>({
  value,
  placeholder,
  options,
  onChange,
  disabled = false,
}: DropdownSelectProps<T>) {
  const [isOpen, setIsOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    const handlePointerDown = (e: MouseEvent) => {
      if (!containerRef.current?.contains(e.target as Node)) setIsOpen(false);
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setIsOpen(false);
    };
    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [isOpen]);

  const selected = options.find((o) => o.value === value);

  return (
    <div ref={containerRef} className="relative">
      <button
        type="button"
        onClick={() => setIsOpen((o) => !o)}
        disabled={disabled}
        className="flex w-full items-center justify-between gap-2 rounded-2xl bg-slate-50 py-2.5 pl-4 pr-3 text-sm font-medium text-slate-700 disabled:cursor-not-allowed disabled:opacity-60"
      >
        <span className="truncate">{selected ? selected.label : placeholder}</span>
        <ChevronDown className={`h-4 w-4 shrink-0 text-slate-400 transition-transform ${isOpen ? "rotate-180" : ""}`} />
      </button>

      {isOpen && (
        <div className="absolute left-0 right-0 top-full z-20 mt-2 overflow-hidden rounded-2xl bg-white p-1.5 shadow-soft-lg ring-1 ring-slate-900/[0.06]">
          <ul className="max-h-64 divide-y divide-slate-100 overflow-y-auto">
            {options.map((option) => {
              const isSelected = option.value === value;
              return (
                <li key={option.value}>
                  <button
                    type="button"
                    disabled={option.disabled}
                    onClick={() => {
                      onChange(option.value);
                      setIsOpen(false);
                    }}
                    className={[
                      "flex w-full items-center justify-between gap-3 rounded-xl px-3.5 py-2.5 text-left text-sm transition-colors",
                      option.disabled
                        ? "cursor-not-allowed text-slate-300"
                        : isSelected
                          ? "bg-brand-50 text-brand-700"
                          : "text-slate-700 hover:bg-slate-50",
                    ].join(" ")}
                  >
                    <span className="flex min-w-0 items-center gap-2">
                      <Check className={`h-3.5 w-3.5 shrink-0 ${isSelected ? "text-brand-600" : "text-transparent"}`} />
                      <span className="truncate font-medium">{option.label}</span>
                    </span>
                    {option.meta && <span className="shrink-0">{option.meta}</span>}
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
}
