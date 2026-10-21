import type { CreateAndHoldOrderInput, Order, OrdersPayInput, Sale } from "../types";
import { call } from "./tauriClient";

/** Creates an order, adds lines, and holds it in the active orders queue. */
export function createAndHoldOrder(input: CreateAndHoldOrderInput): Promise<Order> {
  return call<Order>("orders_create_and_hold", { input });
}

/** Lists all open orders (takeaway, dine-in) that are not yet completed or cancelled. */
export function listActiveOrders(): Promise<Order[]> {
  return call<Order[]>("orders_list_active", {});
}

/** Fetches full details for a single order by id. */
export function getOrder(orderId: number): Promise<Order> {
  return call<Order>("orders_get", { orderId });
}

/** Resumes an order held in draft/parking. */
export function resumeOrder(orderId: number, version: number): Promise<Order> {
  return call<Order>("orders_resume", { orderId, version });
}

/** Transitions order fulfillment status: "sent" | "preparing" | "ready" | "served". */
export function transitionOrder(orderId: number, version: number, toStatus: string): Promise<Order> {
  return call<Order>("orders_transition", { orderId, version, toStatus });
}

/** Updates the custom preparation time in minutes for an active order. */
export function updateOrderPrepTime(orderId: number, prepMinutes: number): Promise<Order> {
  return call<Order>("orders_update_prep_time", { orderId, prepMinutes });
}

/** Cancels an order. */
export function cancelOrder(orderId: number, version: number, reason?: string): Promise<Order> {
  return call<Order>("orders_cancel", { orderId, version, reason });
}

/** Settles and pays order lines. */
export function payOrder(input: OrdersPayInput): Promise<{ order: Order; sale: Sale }> {
  return call<{ order: Order; sale: Sale }>("orders_pay", { input });
}

/** Prints a customer token or kitchen ticket for an active order via thermal printer. */
export function printOrderToken(orderId: number, isKitchen = false): Promise<void> {
  return call<void>("orders_print_token", { orderId, isKitchen });
}

/** Updates delivery lifecycle status: "placed" | "dispatched" | "delivered" | "cancelled". */
export function updateDeliveryStatus(orderId: number, deliveryStatus: string): Promise<Order> {
  return call<Order>("orders_update_delivery_status", { orderId, deliveryStatus });
}
