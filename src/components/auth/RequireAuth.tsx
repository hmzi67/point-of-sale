import { Navigate, Outlet } from "react-router-dom";
import { useAuthStore } from "../../store";

/** Gate for every screen behind login. */
export function RequireAuth() {
  const user = useAuthStore((state) => state.user);
  return user ? <Outlet /> : <Navigate to="/login" replace />;
}
