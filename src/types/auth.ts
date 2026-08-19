export type Role = "owner" | "admin" | "cashier";

export interface User {
  id: number;
  name: string;
  role: Role;
}

/** A user as shown on the user management screen — includes whether the
 * account is active, unlike `User` (only ever an active account). */
export interface ManagedUser {
  id: number;
  name: string;
  role: Role;
  isActive: boolean;
}

export const PIN_MIN_LENGTH = 4;
export const PIN_MAX_LENGTH = 6;
