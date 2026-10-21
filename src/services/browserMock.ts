import type { User, ManagedUser } from "../types/auth";
import type { DeliveryPlatform, Order, CreateAndHoldOrderInput, OrdersPayInput } from "../types/orders";
import type { Sale } from "../types/billing";
import type { Item, Category } from "../types/inventory";

const MOCK_USERS: ManagedUser[] = [
  { id: 1, name: "Owner", role: "owner", isActive: true },
  { id: 2, name: "Admin", role: "admin", isActive: true },
  { id: 3, name: "Cashier", role: "cashier", isActive: true },
  { id: 4, name: "Ali Rider", role: "delivery", isActive: true },
];

const MOCK_CATEGORIES: Category[] = [
  { id: 1, name: "Fast Food" },
  { id: 2, name: "Beverages" },
  { id: 3, name: "Sides" },
];

const MOCK_ITEMS: Item[] = [
  {
    id: 1,
    name: "Zinger Burger",
    priceMinor: 45000,
    costMinor: 25000,
    stockQty: 50,
    barcode: "1001",
    shortCode: null,
    description: null,
    categoryId: 1,
    categoryName: "Fast Food",
    lowStockThreshold: 5,
    isActive: true,
    isLowStock: false,
    imagePath: null,
    soldByAmount: false,
    unit: "pcs",
    counterId: null,
    counterName: null,
  },
  {
    id: 2,
    name: "Crispy Fries (Large)",
    priceMinor: 20000,
    costMinor: 8000,
    stockQty: 80,
    barcode: "1002",
    shortCode: null,
    description: null,
    categoryId: 3,
    categoryName: "Sides",
    lowStockThreshold: 5,
    isActive: true,
    isLowStock: false,
    imagePath: null,
    soldByAmount: false,
    unit: "pcs",
    counterId: null,
    counterName: null,
  },
  {
    id: 3,
    name: "Cola 500ml",
    priceMinor: 10000,
    costMinor: 6000,
    stockQty: 120,
    barcode: "1003",
    shortCode: null,
    description: null,
    categoryId: 2,
    categoryName: "Beverages",
    lowStockThreshold: 10,
    isActive: true,
    isLowStock: false,
    imagePath: null,
    soldByAmount: false,
    unit: "pcs",
    counterId: null,
    counterName: null,
  },
];

const MOCK_PLATFORMS: DeliveryPlatform[] = [
  { id: 1, name: "Foodpanda", isActive: true, createdAt: new Date().toISOString() },
  { id: 2, name: "Cheetay", isActive: true, createdAt: new Date().toISOString() },
];

let currentUser: User | null = { id: 1, name: "Owner", role: "owner" };
let mockOrders: Order[] = [];
let nextOrderId = 1;
let nextSaleId = 1;

export async function handleBrowserMock<TResult>(
  command: string,
  args?: Record<string, unknown>,
): Promise<TResult> {
  switch (command) {
    case "app_ping":
      return "pong (browser dev mock)" as unknown as TResult;

    case "users_list_active":
      return MOCK_USERS.filter((u) => u.isActive) as unknown as TResult;

    case "users_list_all":
      return MOCK_USERS as unknown as TResult;

    case "auth_login": {
      const uid = Number(args?.userId);
      const found = MOCK_USERS.find((u) => u.id === uid) || MOCK_USERS[0];
      currentUser = { id: found.id, name: found.name, role: found.role };
      return currentUser as unknown as TResult;
    }

    case "auth_current_user":
      return currentUser as unknown as TResult;

    case "auth_logout":
      currentUser = null;
      return undefined as unknown as TResult;

    case "config_get":
      return {
        currency: "PKR",
        taxPercent: 0,
        storeName: "Diwan Demo POS",
        receiptHeader: "Welcome to Diwan",
        receiptFooter: "Thank you for visiting!",
      } as unknown as TResult;

    case "config_get_ui_prefs":
      return {
        theme: "light",
        fontScale: 1.0,
        glassEffect: false,
        fastBillingEnabled: false,
      } as unknown as TResult;

    case "config_set_ui_prefs":
      return undefined as unknown as TResult;

    case "config_get_enabled_modules":
      return [
        { moduleKey: "tables", isEnabled: true, isLocked: false },
        { moduleKey: "delivery", isEnabled: true, isLocked: false },
        { moduleKey: "reports", isEnabled: true, isLocked: false },
        { moduleKey: "expenses", isEnabled: true, isLocked: false },
        { moduleKey: "shifts", isEnabled: true, isLocked: false },
        { moduleKey: "salary", isEnabled: true, isLocked: false },
        { moduleKey: "attendance", isEnabled: true, isLocked: false },
      ] as unknown as TResult;

    case "license_get_state":
      return {
        isValid: true,
        status: "active",
        expiresAt: "2099-12-31T23:59:59Z",
        daysRemaining: 9999,
        isTrial: false,
      } as unknown as TResult;

    case "license_get_info":
      return {
        licenseKey: "DEMO-KEY-1234",
        storeName: "Diwan Demo Store",
        issuedTo: "Dev Browser Mode",
        expiresAt: "2099-12-31T23:59:59Z",
      } as unknown as TResult;

    case "inventory_get_categories":
      return MOCK_CATEGORIES as unknown as TResult;

    case "inventory_get_items":
      return MOCK_ITEMS as unknown as TResult;

    case "inventory_get_best_selling_item_ids":
      return [1, 2, 3] as unknown as TResult;

    case "delivery_platforms_list":
    case "delivery_platforms_list_active":
      return MOCK_PLATFORMS as unknown as TResult;

    case "delivery_platforms_get_recent_riders":
      return [] as unknown as TResult;

    case "tables_list":
      return [
        { id: 1, name: "Table 1", capacity: 4, status: "free" },
        { id: 2, name: "Table 2", capacity: 6, status: "free" },
      ] as unknown as TResult;

    case "shifts_get_current":
      return null as unknown as TResult;

    case "orders_list_active":
      return mockOrders.filter((o) => o.status !== "cancelled" && !o.completedAt) as unknown as TResult;

    case "orders_get": {
      const oid = Number(args?.orderId);
      const found = mockOrders.find((o) => o.id === oid);
      if (!found) throw new Error(`Order #${oid} not found in mock store`);
      return found as unknown as TResult;
    }

    case "orders_create_and_hold": {
      const input = args?.input as CreateAndHoldOrderInput;
      const id = nextOrderId++;
      const lines = (input.lines || []).map((l, idx) => {
        const item = MOCK_ITEMS.find((it) => it.id === l.itemId);
        return {
          id: idx + 1,
          itemId: l.itemId,
          itemName: item?.name || `Item ${l.itemId}`,
          qty: l.qty,
          priceAtOrderMinor: item?.priceMinor || 10000,
          notes: l.notes || null,
          status: "active" as const,
          sentAt: new Date().toISOString(),
          saleId: null,
        };
      });

      const dueMinor = lines.reduce((sum, l) => sum + l.priceAtOrderMinor * l.qty, 0) + (input.delivery?.deliveryChargeMinor || 0);

      const order: Order = {
        id,
        orderNo: id,
        orderType: (input.orderType as any) || "takeaway",
        tableId: input.tableId || null,
        tableName: input.tableId ? `Table ${input.tableId}` : null,
        status: "held",
        heldFromStatus: "draft",
        version: 3,
        clientRef: input.clientRef || crypto.randomUUID(),
        customerName: input.customerName || null,
        notes: input.notes || null,
        estimatedPrepMinutes: input.estimatedPrepMinutes || 15,
        paymentStatus: "unpaid",
        dueMinor,
        createdAt: new Date().toISOString(),
        completedAt: null,
        cancelledAt: null,
        deliveryRiderId: input.delivery?.deliveryRiderId,
        riderSource: input.delivery?.riderSource,
        deliveryRiderName: input.delivery?.deliveryRiderName,
        deliveryRiderPhone: input.delivery?.deliveryRiderPhone,
        deliveryPlatformId: input.delivery?.deliveryPlatformId,
        deliveryPlatformName: input.delivery?.deliveryPlatformName,
        customerPhone: input.delivery?.customerPhone,
        deliveryAddress: input.delivery?.deliveryAddress,
        deliveryChargeMinor: input.delivery?.deliveryChargeMinor || 0,
        deliveryStatus: input.delivery?.deliveryStatus || (input.orderType === "delivery" ? "placed" : null),
        platformOrderRef: input.delivery?.platformOrderRef,
        lines,
      };
      mockOrders.push(order);
      return order as unknown as TResult;
    }

    case "orders_resume": {
      const oid = Number(args?.orderId);
      const found = mockOrders.find((o) => o.id === oid);
      if (!found) throw new Error(`Order #${oid} not found`);
      found.status = (found.heldFromStatus as any) || "draft";
      found.heldFromStatus = null;
      found.version += 1;
      return { ...found } as unknown as TResult;
    }

    case "orders_transition": {
      const oid = Number(args?.orderId);
      const toStatus = String(args?.toStatus);
      const found = mockOrders.find((o) => o.id === oid);
      if (!found) throw new Error(`Order #${oid} not found`);
      found.status = toStatus as any;
      found.version += 1;
      if (toStatus === "served" && found.paymentStatus === "paid") {
        found.completedAt = new Date().toISOString();
      }
      return { ...found } as unknown as TResult;
    }

    case "orders_update_delivery_status": {
      const oid = Number(args?.orderId);
      const deliveryStatus = String(args?.deliveryStatus);
      const found = mockOrders.find((o) => o.id === oid);
      if (!found) throw new Error(`Order #${oid} not found`);
      found.deliveryStatus = deliveryStatus as any;
      found.version += 1;
      if (deliveryStatus === "delivered" && found.paymentStatus === "paid") {
        found.status = "served";
        found.completedAt = new Date().toISOString();
      }
      return { ...found } as unknown as TResult;
    }

    case "orders_pay": {
      const input = args?.input as OrdersPayInput;
      const found = mockOrders.find((o) => o.id === input.orderId);
      if (!found) throw new Error(`Order #${input.orderId} not found`);

      const saleId = nextSaleId++;
      for (const line of found.lines) {
        if (input.orderItemIds.includes(line.id)) {
          line.saleId = saleId;
        }
      }

      const allPaid = found.lines.every((l) => l.saleId !== null);
      if (allPaid) {
        found.paymentStatus = "paid";
        found.dueMinor = 0;
        // Takeaway orders immediately complete upon payment
        if (found.orderType === "takeaway") {
          found.status = "served";
          found.completedAt = new Date().toISOString();
        } else if ((found.orderType === "delivery" || found.orderType === "third_party_delivery") && found.deliveryStatus === "delivered") {
          found.status = "served";
          found.completedAt = new Date().toISOString();
        }
      }
      found.version += 1;

      const sale: Sale = {
        id: saleId,
        tableId: found.tableId,
        tableName: found.tableName,
        shiftId: null,
        totalMinor: found.lines.reduce((s, l) => s + l.priceAtOrderMinor * l.qty, 0),
        subtotalMinor: found.lines.reduce((s, l) => s + l.priceAtOrderMinor * l.qty, 0),
        taxMinor: input.taxMinor || 0,
        discountMinor: input.discountMinor || 0,
        paymentMethod: (input.paymentMethod as any) || "cash",
        createdAt: new Date().toISOString(),
        cashierId: currentUser?.id || 1,
        cashierName: currentUser?.name || "Owner",
        items: found.lines.map((l) => ({
          id: l.id,
          itemId: l.itemId,
          itemName: l.itemName,
          qty: l.qty,
          unit: "pcs",
          priceAtSaleMinor: l.priceAtOrderMinor,
          lineTotalMinor: l.priceAtOrderMinor * l.qty,
          notes: l.notes,
        })),
      };

      return { order: { ...found }, sale } as unknown as TResult;
    }

    case "billing_create_sale": {
      const saleId = nextSaleId++;
      const sale: Sale = {
        id: saleId,
        tableId: null,
        tableName: null,
        shiftId: null,
        totalMinor: 50000,
        subtotalMinor: 50000,
        taxMinor: 0,
        discountMinor: 0,
        paymentMethod: "cash",
        createdAt: new Date().toISOString(),
        cashierId: currentUser?.id || 1,
        cashierName: currentUser?.name || "Owner",
        items: [],
      };
      return sale as unknown as TResult;
    }

    case "orders_print_token":
    case "printer_print_diagnostic":
      return undefined as unknown as TResult;

    case "dashboard_get_summary":
      return {
        todaySalesMinor: 150000,
        todaySalesCount: 12,
        activeOrdersCount: mockOrders.filter((o) => !o.completedAt).length,
        lowStockCount: 0,
      } as unknown as TResult;

    default:
      // Graceful fallback for unmocked commands in browser preview
      console.warn(`[BrowserMock] Unhandled command: ${command}`);
      return null as unknown as TResult;
  }
}
