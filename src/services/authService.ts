import type { ManagedUser, Role, User } from "../types";
import { call } from "./tauriClient";

/** Accounts offered on the login screen. Never carries PIN hashes. */
export function getUsers(): Promise<User[]> {
  return call<User[]>("get_users");
}

export function login(userId: number, pin: string): Promise<User> {
  return call<User>("login", { userId, pin });
}

/** Clears the server-side session. Safe to call even if it turns out nothing
 * was signed in — never throws, so a logout button never gets stuck. */
export function logout(): Promise<void> {
  return call<void>("logout");
}

export function createUser(name: string, pin: string, role: Role): Promise<User> {
  return call<User>("create_user", { name, pin, role });
}

export function setUserPin(userId: number, pin: string): Promise<void> {
  return call<void>("set_user_pin", { userId, pin });
}

/** Every account, active or not — the user management screen. Owner/Admin
 * only; the backend re-checks this regardless of who calls it. */
export function getAllUsers(): Promise<ManagedUser[]> {
  return call<ManagedUser[]>("get_all_users");
}

export function updateUser(userId: number, name: string, role: Role): Promise<User> {
  return call<User>("update_user", { userId, name, role });
}

export function setUserActive(userId: number, isActive: boolean): Promise<void> {
  return call<void>("set_user_active", { userId, isActive });
}
