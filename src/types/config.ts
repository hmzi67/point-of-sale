export type BusinessType = "retail" | "restaurant" | "other";

export interface AppConfig {
  businessName: string;
  businessType: BusinessType;
  logoPath: string | null;
  currency: string;
  taxPercent: number;
  receiptFooter: string;
  /** Divisor for salary calculation — see `db/salary.rs`. */
  workingDaysPerMonth: number;
}
