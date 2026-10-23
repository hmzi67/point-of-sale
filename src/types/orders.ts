export interface OrderLine {
  id: number;
  itemId: number;
  itemName: string;
  qty: number;
  priceAtOrderMinor: number;
  notes: string | null;
  status: "active" | "voided";
  sentAt: string | null;
  saleId: number | null;
}

export type OrderType = "takeaway" | "dine_in" | "delivery" | "third_party_delivery";
export type OrderStatus = "draft" | "sent" | "preparing" | "ready" | "served" | "held" | "cancelled";
export type OrderPaymentStatus = "unpaid" | "partial" | "paid";
export type DeliveryStatus = "placed" | "dispatched" | "delivered" | "cancelled";

export interface DeliveryDetails {
  deliveryRiderId?: number | null;
  riderSource?: "user" | "manual" | null;
  deliveryRiderName?: string | null;
  deliveryRiderPhone?: string | null;
  deliveryPlatformId?: number | null;
  deliveryPlatformName?: string | null;
  customerPhone?: string | null;
  deliveryAddress?: string | null;
  deliveryChargeMinor?: number | null;
  deliveryStatus?: DeliveryStatus | null;
  platformOrderRef?: string | null;
}

export interface DeliveryPlatform {
  id: number;
  name: string;
  isActive: boolean;
  createdAt: string;
}

export interface RecentRider {
  name: string;
  phone: string;
}

export interface Order {
  id: number;
  orderNo: number;
  orderType: OrderType;
  tableId: number | null;
  tableName: string | null;
  status: OrderStatus;
  heldFromStatus: string | null;
  version: number;
  clientRef: string;
  customerName: string | null;
  notes: string | null;
  estimatedPrepMinutes: number;
  paymentStatus: OrderPaymentStatus;
  dueMinor: number;
  createdAt: string;
  completedAt: string | null;
  cancelledAt: string | null;
  shiftId?: number | null;
  deliveryRiderId?: number | null;
  riderSource?: "user" | "manual" | null;
  deliveryRiderName?: string | null;
  deliveryRiderPhone?: string | null;
  deliveryPlatformId?: number | null;
  deliveryPlatformName?: string | null;
  customerPhone?: string | null;
  deliveryAddress?: string | null;
  deliveryChargeMinor?: number;
  deliveryStatus?: DeliveryStatus | null;
  platformOrderRef?: string | null;
  lines: OrderLine[];
}

export interface NewOrderLineInput {
  itemId: number;
  qty: number;
  notes?: string | null;
}

export interface CreateAndHoldOrderInput {
  clientRef: string;
  orderType: string;
  tableId?: number | null;
  customerName?: string | null;
  notes?: string | null;
  estimatedPrepMinutes?: number | null;
  delivery?: DeliveryDetails | null;
  lines: NewOrderLineInput[];
}

export interface OrdersPayInput {
  orderId: number;
  version: number;
  orderItemIds: number[];
  paymentMethod: string;
  discountMinor: number;
  taxMinor: number;
  tenderedMinor?: number | null;
  changeMinor?: number | null;
  payments?: import("./billing").PaymentPart[];
  customerId?: number | null;
  serviceChargeMinor?: number;
  dueDate?: string | null;
}
