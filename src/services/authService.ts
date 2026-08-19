import type { Role, User } from "../types";
import { call } from "./tauriClient";

/** Accounts offered on the login screen. Never carries PIN hashes. */
export function getUsers(): Promise<User[]> {
  return call<User[]>("get_users");
}

export function login(userId: number, pin: string): Promise<User> {
  return call<User>("login", { userId, pin });
}

export function createUser(name: string, pin: string, role: Role): Promise<User> {
  return call<User>("create_user", { name, pin, role });
}

export function setUserPin(userId: number, pin: string): Promise<void> {
  return call<void>("set_user_pin", { userId, pin });
}
