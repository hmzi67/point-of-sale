import { useNavigate } from "react-router-dom";
import { Play, Power, Wallet } from "lucide-react";
import { useShiftStore } from "../../store";
import type { AppConfig } from "../../types";
import { ModalCard } from "../ui/ModalCard";
import { CloseShiftModal } from "./CloseShiftModal";
import { OpenShiftModal } from "./OpenShiftModal";

/** `"choose"`: ask Continue / Close. `{ closeShiftId }`: the close-out flow,
 * kept by id because the store forgets the shift as soon as it closes. */
export type ShiftStage = "choose" | { closeShiftId: number } | null;

interface ShiftGateProps {
  config: AppConfig;
  stage: ShiftStage;
  onStageChange: (stage: ShiftStage) => void;
}

/**
 * Billing is always worked inside a shift (the `shifts` module on). Every
 * visit to Billing starts at `"choose"`:
 * - no open shift → only "Start shift" (cannot be skipped; "Leave billing" exits),
 * - an open one → "Continue shift" or "Close shift".
 * Closing shows the reconciliation, prints it, and lands back on "Start shift".
 */
export function ShiftGate({ config, stage, onStageChange }: ShiftGateProps) {
  const openShift = useShiftStore((state) => state.openShift);
  const isLoaded = useShiftStore((state) => state.isLoaded);
  const navigate = useNavigate();

  if (!isLoaded) return null;

  if (stage !== null && typeof stage === "object") {
    return <CloseShiftModal shiftId={stage.closeShiftId} config={config} onClose={() => onStageChange(null)} />;
  }

  if (!openShift) {
    return (
      <OpenShiftModal
        currency={config.currency}
        mandatory
        onClose={() => navigate("/")}
        onOpened={() => onStageChange(null)}
      />
    );
  }

  if (stage !== "choose") return null;

  const openedAt = new Date(openShift.openedAt.replace(" ", "T"));
  return (
    <ModalCard
      title={`Shift #${openShift.id} is open`}
      subtitle={`Started ${openedAt.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}${openShift.cashierName ? ` by ${openShift.cashierName}` : ""}`}
      icon={Wallet}
      width="sm"
      onClose={() => onStageChange(null)}
    >
      <div className="grid gap-2">
        <button
          type="button"
          autoFocus
          onClick={() => onStageChange(null)}
          className="flex items-center justify-center gap-2 rounded-xl bg-accent px-4 py-3 text-sm font-semibold text-accent-ink hover:bg-accent-hover"
        >
          <Play className="h-4 w-4" />
          Continue shift
        </button>
        <button
          type="button"
          onClick={() => onStageChange({ closeShiftId: openShift.id })}
          className="flex items-center justify-center gap-2 rounded-xl border border-line bg-surface-2 px-4 py-3 text-sm font-semibold text-fg hover:bg-surface-3"
        >
          <Power className="h-4 w-4 text-danger-text" />
          Close shift
        </button>
      </div>
    </ModalCard>
  );
}
