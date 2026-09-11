import { Component, type ErrorInfo, type ReactNode } from "react";

interface RootErrorBoundaryProps {
  children: ReactNode;
}

interface RootErrorBoundaryState {
  error: Error | null;
}

/**
 * The one and only error boundary in the app, wrapping everything in
 * `main.tsx`. Without this, an uncaught error during initial render (a bad
 * prop, a hook throwing, anything before the router even mounts) left React
 * to unmount the whole tree per its default behavior — a blank white
 * window with zero explanation, no different in effect from the native
 * "opens and closes" failure this whole diagnostics pass exists to fix,
 * just one layer up the stack (JS instead of Rust).
 *
 * Deliberately not fancy — this is the *last* line of defense, not a
 * normal error-handling path, so it doesn't reach for any app UI
 * primitives that might themselves be implicated in whatever broke.
 */
export class RootErrorBoundary extends Component<RootErrorBoundaryProps, RootErrorBoundaryState> {
  state: RootErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): RootErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // eslint-disable-next-line no-console
    console.error("RootErrorBoundary caught:", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;

    return (
      <div
        style={{
          display: "flex",
          height: "100dvh",
          alignItems: "center",
          justifyContent: "center",
          padding: 24,
          fontFamily: "sans-serif",
          // Inline styles (this renders when React's tree may be broken, so
          // no Tailwind classes) — but the theme variables on <html> are
          // plain CSS and still resolve. Fallbacks are the dark values.
          background: "var(--t-bg, #0a0f0d)",
          color: "var(--t-fg, #f2f6f4)",
        }}
      >
        <div style={{ maxWidth: 480, textAlign: "center" }}>
          <h1 style={{ fontSize: 18, fontWeight: 600, marginBottom: 8 }}>Diwan couldn't start</h1>
          <p style={{ fontSize: 13, color: "var(--t-fg-muted, #93a39b)", marginBottom: 16 }}>
            Something went wrong before the app could load. Restarting usually fixes this — if it keeps happening,
            share this message.
          </p>
          <pre
            style={{
              fontSize: 11,
              textAlign: "left",
              background: "var(--t-surface-2, #18221e)",
              border: "1px solid var(--t-line, #1e2a25)",
              padding: 12,
              borderRadius: 8,
              overflowX: "auto",
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
            }}
          >
            {this.state.error.message}
          </pre>
        </div>
      </div>
    );
  }
}
