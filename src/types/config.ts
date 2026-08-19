export type BusinessType = "retail" | "restaurant" | "other";

export interface AppConfig {
  businessName: string;
  businessType: BusinessType;
  logoPath: string | null;
  currency: string;
  taxPercent: number;
  receiptFooter: string;
}
