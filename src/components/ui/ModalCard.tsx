import type { ReactNode } from "react";
import { X, type LucideIcon } from "lucide-react";
import { ModalOverlay } from "./ModalOverlay";

const WIDTHS = { sm: "max-w-sm", md: "max-w-lg", lg: "max-w-3xl", xl: "max-w-5xl" };

/** A dialog card on the shared scrim: title row with a close button, then a
 * scrollable body. */
export function ModalCard({
  title,
  subtitle,
  icon,
  width = "md",
  onClose,
  children,
}: {
  title: string;
  subtitle?: string;
  icon?: LucideIcon;
  width?: keyof typeof WIDTHS;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <ModalOverlay>
      <div className={`flex max-h-[92vh] w-full ${WIDTHS[width]} flex-col rounded-2xl border border-line bg-surface shadow-xl`}>
        <ModalHeader title={title} subtitle={subtitle} icon={icon} onClose={onClose} className="px-5 py-4" />
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">{children}</div>
      </div>
    </ModalOverlay>
  );
}

/** A dialog's title row: optional icon badge, title, subtitle and the close
 * button, over a bottom rule. `className` sets the padding. */
export function ModalHeader({
  title,
  subtitle,
  icon: Icon,
  onClose,
  className = "",
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  icon?: LucideIcon;
  onClose: () => void;
  className?: string;
}) {
  return (
    <div className={`flex items-center justify-between gap-3 border-b border-line ${className}`}>
      <div className="flex min-w-0 items-center gap-2.5">
        {Icon && (
          <span className="shrink-0 rounded-xl bg-accent-soft p-2 text-accent-text">
            <Icon className="h-5 w-5" />
          </span>
        )}
        <div className="min-w-0">
          <h3 className={`${Icon ? "text-lg" : "text-base"} font-semibold tracking-tight text-fg`}>{title}</h3>
          {subtitle && <p className="mt-0.5 text-xs text-fg-muted">{subtitle}</p>}
        </div>
      </div>
      <button
        type="button"
        onClick={onClose}
        aria-label="Close"
        className="shrink-0 rounded-lg p-1.5 text-fg-muted hover:bg-surface-2 hover:text-fg-2"
      >
        <X className="h-4 w-4" />
      </button>
    </div>
  );
}

/** The standard label + control pair used inside dialogs. */
export function Field({ label, children, className = "" }: { label: string; children: ReactNode; className?: string }) {
  return (
    <label className={`block ${className}`}>
      <span className="block text-xs font-medium text-fg-muted">{label}</span>
      <div className="mt-1">{children}</div>
    </label>
  );
}

export const inputClass = "w-full rounded-xl border border-line-strong bg-surface px-3 py-2.5 text-sm";
export const primaryButtonClass =
  "rounded-xl bg-accent px-3.5 py-2 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50";
export const secondaryButtonClass =
  "rounded-xl border border-line-strong px-3.5 py-2 text-sm font-medium text-fg-2 hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-50";
