export type Role = "owner" | "admin" | "cashier";

export interface User {
  id: number;
  name: string;
  role: Role;
}

export const PIN_MIN_LENGTH = 4;
export const PIN_MAX_LENGTH = 6;
