import { AlertTriangle } from "lucide-react";
import { ModalOverlay } from "./ModalOverlay";

interface ConfirmDialogProps {
  title: string;
  message: string;
  confirmLabel?: string;
  isDangerous?: boolean;
  isBusy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  title,
  message,
  confirmLabel = "Confirm",
  isDangerous = false,
  isBusy = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  return (
    <ModalOverlay>
      <div className="w-full max-w-sm rounded-2xl border border-line bg-surface glass p-6 shadow-soft-lg">
        <div className="flex items-start gap-3">
          {isDangerous && (
            <span className="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-danger-soft">
              <AlertTriangle className="h-5 w-5 text-danger-text" />
            </span>
          )}
          <div>
            <h3 className="text-base font-semibold text-fg">{title}</h3>
            <p className="mt-1 text-sm text-fg-2">{message}</p>
          </div>
        </div>

        <div className="mt-6 flex justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            disabled={isBusy}
            className="rounded-md px-3 py-1.5 text-sm font-medium text-fg-2 hover:bg-surface-3 disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={isBusy}
            className={[
              "rounded-md px-3 py-1.5 text-sm font-medium disabled:opacity-50",
              isDangerous
                ? "bg-danger text-white hover:bg-danger/85"
                : "bg-accent text-accent-ink hover:bg-accent-hover",
            ].join(" ")}
          >
            {isBusy ? "…" : confirmLabel}
          </button>
        </div>
      </div>
    </ModalOverlay>
  );
}
