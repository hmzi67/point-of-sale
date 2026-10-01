import type { ManagedUser, Role, User } from "../types";
import { call } from "./tauriClient";

/** Accounts offered on the login screen. Never carries PIN hashes. */
export function getUsers(): Promise<User[]> {
  return call<User[]>("users_list_active");
}

export function login(userId: number, pin: string): Promise<User> {
  return call<User>("auth_login", { userId, pin });
}

/** Clears the server-side session. Safe to call even if it turns out nothing
 * was signed in — never throws, so a logout button never gets stuck. */
export function logout(): Promise<void> {
  return call<void>("auth_logout");
}

export function createUser(name: string, pin: string, role: Role): Promise<User> {
  return call<User>("users_create", { name, pin, role });
}

export function setUserPin(userId: number, pin: string): Promise<void> {
  return call<void>("users_set_pin", { userId, pin });
}

/** Every account, active or not — the user management screen. Owner/Admin
 * only; the backend re-checks this regardless of who calls it. */
export function getAllUsers(): Promise<ManagedUser[]> {
  return call<ManagedUser[]>("users_list_all");
}

export function updateUser(userId: number, name: string, role: Role): Promise<User> {
  return call<User>("users_update", { userId, name, role });
}

export function setUserActive(userId: number, isActive: boolean): Promise<void> {
  return call<void>("users_set_active", { userId, isActive });
}
