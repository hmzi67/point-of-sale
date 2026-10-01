/**
 * The busy-flag + error-line wrapper most buttons that call a write share:
 * sets `setBusy(true)` and clears the error, runs `action`, reports a
 * failure's message (optionally prefixed) into `setError`, and always clears
 * the busy flag. Resolves `true` on success so a caller can follow up
 * (close a dialog, reload) only when the action actually went through.
 */
export async function runAction(
  setBusy: (busy: boolean) => void,
  setError: (message: string | null) => void,
  action: () => Promise<unknown>,
  errorPrefix?: string,
): Promise<boolean> {
  setBusy(true);
  setError(null);
  try {
    await action();
    return true;
  } catch (e) {
    const message = (e as Error).message;
    setError(errorPrefix ? `${errorPrefix}: ${message}` : message);
    return false;
  } finally {
    setBusy(false);
  }
}
