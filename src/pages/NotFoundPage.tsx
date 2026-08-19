import { Link } from "react-router-dom";

export function NotFoundPage() {
  return (
    <section className="mx-auto max-w-md text-center">
      <h2 className="text-2xl font-semibold text-slate-900">Page not found</h2>
      <p className="mt-2 text-sm text-slate-600">
        That screen does not exist, or its module is turned off for this installation.
      </p>
      <Link
        to="/"
        className="mt-6 inline-block rounded-md bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-700"
      >
        Back to dashboard
      </Link>
    </section>
  );
}
