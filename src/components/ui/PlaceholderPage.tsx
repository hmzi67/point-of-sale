import type { ReactNode } from "react";

interface PlaceholderPageProps {
  title: string;
  description: string;
  phase: string;
  children?: ReactNode;
}

/** Temporary stand-in for module screens until their phase is built. */
export function PlaceholderPage({ title, description, phase, children }: PlaceholderPageProps) {
  return (
    <section className="mx-auto max-w-3xl">
      <div className="rounded-lg border border-line bg-surface p-8">
        <span className="inline-block rounded-full bg-accent-soft px-2.5 py-1 text-xs font-medium text-accent-text">
          {phase}
        </span>
        <h2 className="mt-3 text-xl font-semibold text-fg">{title}</h2>
        <p className="mt-2 text-sm text-fg-2">{description}</p>
        {children}
      </div>
    </section>
  );
}
