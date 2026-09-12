import { useEffect, useState } from "react";
import { Lock, LogOut, ShieldAlert, Unlock, X } from "lucide-react";
import * as productOwnerService from "../../services/productOwnerService";
import { useModuleStore } from "../../store";
import { PLATFORM } from "../../types";
import type { ModuleState } from "../../types";

interface ProductOwnerModalProps {
  onClose: () => void;
}

type Stage = "loading" | "setup" | "login" | "panel";

/**
 * The hidden entry point's actual UI — reached only via
 * `useSecretTapTrigger`, never from any visible button or menu. Deliberately
 * plain: no "Product Owner" marketing chrome, no claim that this is
 * unbreakable (it isn't, against someone with filesystem access to the
 * machine — see docs/SUPPORT.md). On mount it tries to load the module list
 * directly first: if an elevated session from an earlier tap-trigger in
 * this same app run is still valid, that succeeds and skips straight past
 * the password prompt; if not, it falls back to asking for the credential
 * (or, on a fresh install, to setting one).
 */
export function ProductOwnerModal({ onClose }: ProductOwnerModalProps) {
  const [isUpdating, setIsUpdating] = useState(false);
  const [stage, setStage] = useState<Stage>("loading");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [modules, setModules] = useState<ModuleState[]>([]);

  const loadModules = () => {
    productOwnerService
      .getModules("desktop")
      .then((list) => {
        setModules(list);
        setStage("panel");
      })
      .catch(() => {
        // No valid session yet — fall back to figuring out setup vs login.
        productOwnerService
          .getStatus()
          .then((hasAccount) => setStage(hasAccount ? "login" : "setup"))
          .catch((e: Error) => setError(e.message));
      });
  };

  useEffect(loadModules, []);

  const submitSetup = async () => {
    setError(null);
    if (password.length < 8) {
      setError("Password must be at least 8 characters");
      return;
    }
    if (password !== confirmPassword) {
      setError("Passwords don't match");
      return;
    }
    setIsSubmitting(true);
    try {
      await productOwnerService.setup(password);
      setPassword("");
      setConfirmPassword("");
      loadModules();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsSubmitting(false);
    }
  };

  const submitLogin = async () => {
    setError(null);
    setIsSubmitting(true);
    try {
      await productOwnerService.login(password);
      setPassword("");
      loadModules();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsSubmitting(false);
    }
  };

  const setModule = async (moduleKey: string, platform: "desktop" | "android", patch: { enabled?: boolean; locked?: boolean }) => {
    setError(null);
    setIsUpdating(true);
    try {
      const list = await productOwnerService.setModule(
        moduleKey,
        platform,
        patch.enabled ?? null,
        patch.locked ?? null,
      );
      setModules(list);
      useModuleStore.setState({ modules: list.map((module) => ({ ...module, enabled: PLATFORM === "android" ? module.androidEnabled : module.desktopEnabled, locked: PLATFORM === "android" ? module.androidLocked : module.desktopLocked })), isLoaded: true });
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsUpdating(false);
    }
  };

  const endSession = async () => {
    await productOwnerService.logout().catch(() => undefined);
    onClose();
  };

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-scrim p-4">
      <div className="w-full max-w-md overflow-hidden rounded-lg border border-line-strong bg-surface text-fg shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-line-strong px-4 py-3">
          <span className="flex items-center gap-2 text-sm font-semibold">
            <ShieldAlert className="h-4 w-4 text-warning-text" />
            Vendor Access
          </span>
          <button type="button" onClick={onClose} className="rounded p-1 text-fg-muted hover:bg-surface-3 hover:text-fg">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="max-h-[75dvh] overflow-y-auto p-4">
          {stage === "loading" && <p className="text-sm text-fg-muted">Checking…</p>}

          {stage === "setup" && (
            <div className="space-y-3">
              <p className="text-xs text-fg-muted">
                No credential is set on this install yet. Set one now — it is stored only on this
                machine and cannot be recovered from within the app if lost.
              </p>
              <input
                type="password"
                autoFocus
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="New password (min 8 characters)"
                className="w-full rounded border border-line-strong bg-surface-2 px-3 py-2 text-sm placeholder:text-fg-muted focus:border-warning-line focus:outline-none"
              />
              <input
                type="password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && void submitSetup()}
                placeholder="Confirm password"
                className="w-full rounded border border-line-strong bg-surface-2 px-3 py-2 text-sm placeholder:text-fg-muted focus:border-warning-line focus:outline-none"
              />
              {error && <p className="text-xs text-danger-text">{error}</p>}
              <button
                type="button"
                onClick={() => void submitSetup()}
                disabled={isSubmitting}
                className="w-full rounded bg-warning py-2 text-sm font-semibold text-fg hover:bg-warning disabled:opacity-50"
              >
                {isSubmitting ? "Setting…" : "Set Credential"}
              </button>
            </div>
          )}

          {stage === "login" && (
            <div className="space-y-3">
              <input
                type="password"
                autoFocus
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && void submitLogin()}
                placeholder="Password"
                className="w-full rounded border border-line-strong bg-surface-2 px-3 py-2 text-sm placeholder:text-fg-muted focus:border-warning-line focus:outline-none"
              />
              {error && <p className="text-xs text-danger-text">{error}</p>}
              <button
                type="button"
                onClick={() => void submitLogin()}
                disabled={isSubmitting}
                className="w-full rounded bg-warning py-2 text-sm font-semibold text-fg hover:bg-warning disabled:opacity-50"
              >
                {isSubmitting ? "Checking…" : "Unlock"}
              </button>
            </div>
          )}

          {stage === "panel" && (
            <div className="space-y-2">
              <p className="mb-1 text-xs text-fg-muted">
                Locking a module here blocks the client's own Owner/Admin from changing it in Settings.
              </p>
              {error && <p className="text-xs text-danger-text">{error}</p>}
              <fieldset disabled={isUpdating}><ul className="divide-y divide-line">
                {modules.map((m) => (
                  <li key={m.key} className="py-2.5">
                    <p className="text-sm font-medium">
                      {m.name} <span className="text-xs text-fg-muted">({m.key})</span>
                      {m.isCore && <span className="ml-1.5 text-xs text-fg-muted">core</span>}
                    </p>
                    <div className="mt-1.5 flex flex-wrap gap-x-4 gap-y-1 text-xs">
                      <PlatformControls
                        label="Desktop"
                        enabled={m.desktopEnabled}
                        locked={m.desktopLocked}
                        onSetEnabled={(enabled) => void setModule(m.key, "desktop", { enabled })}
                        onToggleLock={() => void setModule(m.key, "desktop", { locked: !m.desktopLocked })}
                      />
                      <PlatformControls
                        label="Android"
                        enabled={m.androidEnabled}
                        locked={m.androidLocked}
                        onSetEnabled={(enabled) => void setModule(m.key, "android", { enabled })}
                        onToggleLock={() => void setModule(m.key, "android", { locked: !m.androidLocked })}
                      />
                    </div>
                  </li>
                ))}
              </ul></fieldset>

              <button
                type="button"
                onClick={() => void endSession()}
                className="mt-2 flex w-full items-center justify-center gap-1.5 rounded border border-line-strong py-2 text-xs font-medium text-fg-muted hover:bg-surface-3"
              >
                <LogOut className="h-3.5 w-3.5" />
                End session
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function PlatformControls({
  label,
  enabled,
  locked,
  onSetEnabled,
  onToggleLock,
}: {
  label: string;
  enabled: boolean;
  locked: boolean;
  onSetEnabled: (enabled: boolean) => void;
  onToggleLock: () => void;
}) {
  return (
    <span className="flex items-center gap-1.5 rounded bg-surface-2 px-2 py-1">
      <span className="text-fg-muted">{label}:</span>
      <button
        type="button"
        onClick={() => onSetEnabled(!enabled)}
        className={enabled ? "font-semibold text-success-text" : "font-semibold text-fg-muted"}
      >
        {enabled ? "on" : "off"}
      </button>
      <button
        type="button"
        onClick={onToggleLock}
        title={locked ? "Locked — click to unlock" : "Unlocked — click to lock"}
        className={locked ? "text-warning-text" : "text-fg-2 hover:text-fg-muted"}
      >
        {locked ? <Lock className="h-3 w-3" /> : <Unlock className="h-3 w-3" />}
      </button>
    </span>
  );
}
