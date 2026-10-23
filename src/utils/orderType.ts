import type { OrderType } from "../types";

/** Display names for an order type wherever it is shown as plain text. */
export const ORDER_TYPE_LABELS: Record<OrderType, string> = {
  dine_in: "Dine-In",
  takeaway: "Takeaway",
  delivery: "Delivery",
  third_party_delivery: "3rd-Party Delivery",
};

/** Short label for a third-party delivery whose platform isn't named —
 * matches the receipt (`printer::escpos::order_field`). */
export const THIRD_PARTY_FALLBACK_LABEL = "3rd-Party";

/** Prep-time target for an order that has none set — mirrors the
 * `orders.estimated_prep_minutes` column default. */
export const DEFAULT_PREP_MINUTES = 15;

/** Own-rider and third-party delivery both follow the delivery status flow
 * (mirrors `db::orders::is_delivery`). */
export function isDeliveryOrder(orderType: string | null | undefined): boolean {
  return orderType === "delivery" || orderType === "third_party_delivery";
}
