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
          background: "#0f172a",
          color: "#f1f5f9",
        }}
      >
        <div style={{ maxWidth: 480, textAlign: "center" }}>
          <h1 style={{ fontSize: 18, fontWeight: 600, marginBottom: 8 }}>POS couldn't start</h1>
          <p style={{ fontSize: 13, color: "#94a3b8", marginBottom: 16 }}>
            Something went wrong before the app could load. Restarting usually fixes this — if it keeps happening,
            share this message.
          </p>
          <pre
            style={{
              fontSize: 11,
              textAlign: "left",
              background: "#1e293b",
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
