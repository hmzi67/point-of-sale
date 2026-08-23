import { useEffect } from "react";
import { HashRouter, Navigate, Route, Routes } from "react-router-dom";
import { ModuleRoute } from "./components/auth/ModuleRoute";
import { RequireAuth } from "./components/auth/RequireAuth";
import { RequireOnboarding } from "./components/auth/RequireOnboarding";
import { AppLayout } from "./components/layout/AppLayout";
import { useAppStore, useAuthStore, useModuleStore } from "./store";
import {
  AttendancePage,
  BillingPage,
  DashboardPage,
  EmployeesPage,
  ExpensesPage,
  InventoryPage,
  LoginPage,
  NotFoundPage,
  OnboardingPage,
  ReportsPage,
  SalaryPage,
  SettingsPage,
  ShiftsPage,
  TablesPage,
  UserManagementPage,
} from "./pages";

/**
 * Loads the installation config once someone is signed in. Doing it after login
 * (rather than at boot) keeps the login screen usable even if config reads fail.
 */
function useBootstrap() {
  const userId = useAuthStore((state) => state.user?.id ?? null);
  const loadConfig = useAppStore((state) => state.load);
  const loadModules = useModuleStore((state) => state.load);

  useEffect(() => {
    if (userId === null) return;
    void loadConfig();
    void loadModules();
  }, [userId, loadConfig, loadModules]);
}

export function App() {
  useBootstrap();

  // Hash routing: Tauri serves the bundle over a custom protocol in production,
  // where a reload on a deep path would otherwise 404.
  return (
    <HashRouter>
      <Routes>
        <Route path="/login" element={<LoginPage />} />

        <Route element={<RequireAuth />}>
          {/* Reachable as soon as someone's logged in, regardless of
              onboarding status — it's the only way out of "not onboarded". */}
          <Route path="onboarding" element={<OnboardingPage />} />

          {/* Everything else waits on setup having actually finished. */}
          <Route element={<RequireOnboarding />}>
            <Route element={<AppLayout />}>
              {/* Every module screen goes through the same guard: enabled for
                  this installation AND permitted for the signed-in role. */}
              <Route
                index
                element={
                  <ModuleRoute moduleKey="dashboard">
                    <DashboardPage />
                  </ModuleRoute>
                }
              />
              <Route
                path="billing"
                element={
                  <ModuleRoute moduleKey="billing">
                    <BillingPage />
                  </ModuleRoute>
                }
              />
              <Route
                path="inventory"
                element={
                  <ModuleRoute moduleKey="inventory">
                    <InventoryPage />
                  </ModuleRoute>
                }
              />
              <Route
                path="reports"
                element={
                  <ModuleRoute moduleKey="reports">
                    <ReportsPage />
                  </ModuleRoute>
                }
              />
              <Route
                path="tables"
                element={
                  <ModuleRoute moduleKey="tables">
                    <TablesPage />
                  </ModuleRoute>
                }
              />
              <Route
                path="attendance"
                element={
                  <ModuleRoute moduleKey="attendance">
                    <AttendancePage />
                  </ModuleRoute>
                }
              />
              <Route
                path="expenses"
                element={
                  <ModuleRoute moduleKey="expenses">
                    <ExpensesPage />
                  </ModuleRoute>
                }
              />
              <Route
                path="salary"
                element={
                  <ModuleRoute moduleKey="salary">
                    <SalaryPage />
                  </ModuleRoute>
                }
              />
              <Route
                path="shifts"
                element={
                  <ModuleRoute moduleKey="shifts">
                    <ShiftsPage />
                  </ModuleRoute>
                }
              />
              <Route
                path="settings"
                element={
                  <ModuleRoute adminOnly>
                    <SettingsPage />
                  </ModuleRoute>
                }
              />
              <Route
                path="users"
                element={
                  <ModuleRoute adminOnly>
                    <UserManagementPage />
                  </ModuleRoute>
                }
              />
              <Route
                path="employees"
                element={
                  <ModuleRoute adminOnly>
                    <EmployeesPage />
                  </ModuleRoute>
                }
              />
              <Route path="404" element={<NotFoundPage />} />
              <Route path="*" element={<Navigate to="/404" replace />} />
            </Route>
          </Route>
        </Route>
      </Routes>
    </HashRouter>
  );
}
