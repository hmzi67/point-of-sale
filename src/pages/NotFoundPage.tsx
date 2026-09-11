import { Link } from "react-router-dom";

export function NotFoundPage() {
  return (
    <section className="mx-auto max-w-md text-center">
      <h2 className="text-2xl font-semibold text-fg">Page not found</h2>
      <p className="mt-2 text-sm text-fg-2">
        That screen does not exist, or its module is turned off for this installation.
      </p>
      <Link
        to="/"
        className="mt-6 inline-block rounded-md bg-accent px-4 py-2 text-sm font-medium text-accent-ink hover:bg-accent-hover"
      >
        Back to dashboard
      </Link>
    </section>
  );
}
