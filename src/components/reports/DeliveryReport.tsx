import { useState } from "react";
import { Bike, DollarSign, PackageCheck, Store, UserCheck } from "lucide-react";
import type { DeliveryReport as DeliveryReportData } from "../../types";
import { formatMinor } from "../../utils/format";

interface DeliveryReportProps {
  data: DeliveryReportData | null;
  currency: string;
  isLoading: boolean;
}

export function DeliveryReport({ data, currency, isLoading }: DeliveryReportProps) {
  const [activeSubTab, setActiveSubTab] = useState<"all" | "platforms" | "riders">("all");

  if (isLoading) {
    return (
      <div className="space-y-6">
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="h-28 animate-pulse rounded-2xl border border-line bg-surface" />
          ))}
        </div>
        <div className="h-64 animate-pulse rounded-2xl border border-line bg-surface" />
      </div>
    );
  }

  if (!data || data.totalOrders === 0) {
    return (
      <div className="rounded-2xl border border-line bg-surface p-12 text-center">
        <Bike className="mx-auto h-10 w-10 stroke-1 text-fg-faint" />
        <h3 className="mt-3 text-base font-semibold text-fg">No Delivery Orders Found</h3>
        <p className="mt-1 text-xs text-fg-muted">
          There are no in-house or third-party delivery orders recorded for the selected period.
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Metric Cards */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        <div className="rounded-2xl border border-line bg-surface p-5">
          <div className="flex items-center justify-between">
            <span className="text-xs font-semibold text-fg-muted">Delivery Orders</span>
            <div className="rounded-xl bg-accent-soft p-2 text-accent-text">
              <PackageCheck className="h-4 w-4" />
            </div>
          </div>
          <p className="mt-2 text-2xl font-bold text-fg">{data.totalOrders}</p>
          <div className="mt-2 flex items-center gap-3 text-xs text-fg-muted">
            <span>In-House: <strong className="text-fg">{data.inHouseOrders}</strong></span>
            <span>·</span>
            <span>3rd-Party: <strong className="text-fg">{data.thirdPartyOrders}</strong></span>
          </div>
        </div>

        <div className="rounded-2xl border border-line bg-surface p-5">
          <div className="flex items-center justify-between">
            <span className="text-xs font-semibold text-fg-muted">Delivery Sales</span>
            <div className="rounded-xl bg-success-soft p-2 text-success-text">
              <DollarSign className="h-4 w-4" />
            </div>
          </div>
          <p className="mt-2 text-2xl font-bold text-fg">
            {formatMinor(data.totalSalesMinor, currency)}
          </p>
          <div className="mt-2 flex items-center gap-3 text-xs text-fg-muted">
            <span>In-House: <strong className="text-fg">{formatMinor(data.inHouseSalesMinor, currency)}</strong></span>
            <span>·</span>
            <span>3rd-Party: <strong className="text-fg">{formatMinor(data.thirdPartySalesMinor, currency)}</strong></span>
          </div>
        </div>

        <div className="rounded-2xl border border-line bg-surface p-5">
          <div className="flex items-center justify-between">
            <span className="text-xs font-semibold text-fg-muted">Delivery Charges Collected</span>
            <div className="rounded-xl bg-info-soft p-2 text-info-text">
              <Bike className="h-4 w-4" />
            </div>
          </div>
          <p className="mt-2 text-2xl font-bold text-fg">
            {formatMinor(data.totalDeliveryChargesMinor, currency)}
          </p>
          <p className="mt-2 text-xs text-fg-muted">From in-house delivery services</p>
        </div>
      </div>

      {/* Sub-tab navigation */}
      <div className="flex gap-2">
        <button
          type="button"
          onClick={() => setActiveSubTab("all")}
          className={`rounded-xl px-3.5 py-2 text-xs font-semibold transition ${
            activeSubTab === "all"
              ? "bg-accent text-accent-ink"
              : "bg-surface-2 text-fg-muted hover:bg-surface-3"
          }`}
        >
          All Orders ({data.orders.length})
        </button>
        <button
          type="button"
          onClick={() => setActiveSubTab("platforms")}
          className={`flex items-center gap-1.5 rounded-xl px-3.5 py-2 text-xs font-semibold transition ${
            activeSubTab === "platforms"
              ? "bg-accent text-accent-ink"
              : "bg-surface-2 text-fg-muted hover:bg-surface-3"
          }`}
        >
          <Store className="h-3.5 w-3.5" />
          Platforms Breakdown ({data.platforms.length})
        </button>
        <button
          type="button"
          onClick={() => setActiveSubTab("riders")}
          className={`flex items-center gap-1.5 rounded-xl px-3.5 py-2 text-xs font-semibold transition ${
            activeSubTab === "riders"
              ? "bg-accent text-accent-ink"
              : "bg-surface-2 text-fg-muted hover:bg-surface-3"
          }`}
        >
          <UserCheck className="h-3.5 w-3.5" />
          Rider Performance ({data.riders.length})
        </button>
      </div>

      {/* Platforms Breakdown Table */}
      {(activeSubTab === "all" || activeSubTab === "platforms") && data.platforms.length > 0 && (
        <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
          <div className="border-b border-line px-5 py-4">
            <h3 className="text-sm font-semibold text-fg">Platform-Wise Summary</h3>
            <p className="mt-0.5 text-xs text-fg-muted">Sales and order breakdown across 3rd-party delivery aggregators.</p>
          </div>
          <table className="w-full text-left text-sm">
            <thead className="border-b border-line bg-surface-2/50 text-xs font-semibold text-fg-2">
              <tr>
                <th className="px-4 py-3.5 font-semibold">Platform</th>
                <th className="px-4 py-3.5 text-center font-semibold">Orders</th>
                <th className="px-4 py-3.5 text-right font-semibold">Total Sales</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {data.platforms.map((p) => (
                <tr key={p.platformId ?? p.platformName}>
                  <td className="px-4 py-3.5 font-medium text-fg">{p.platformName}</td>
                  <td className="px-4 py-3.5 text-center tabular-nums text-fg">{p.orderCount}</td>
                  <td className="px-4 py-3.5 text-right font-bold tabular-nums text-fg">
                    {formatMinor(p.totalSalesMinor, currency)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Rider Performance Table */}
      {(activeSubTab === "all" || activeSubTab === "riders") && data.riders.length > 0 && (
        <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
          <div className="border-b border-line px-5 py-4">
            <h3 className="text-sm font-semibold text-fg">Rider-Wise Performance</h3>
            <p className="mt-0.5 text-xs text-fg-muted">
              Order counts, delivery status, and sales handled per rider (grouped by staff ID or normalized phone).
            </p>
          </div>
          <table className="w-full text-left text-sm">
            <thead className="border-b border-line bg-surface-2/50 text-xs font-semibold text-fg-2">
              <tr>
                <th className="px-4 py-3.5 font-semibold">Rider Name</th>
                <th className="px-4 py-3.5 font-semibold">Phone</th>
                <th className="px-4 py-3.5 font-semibold">Type</th>
                <th className="px-4 py-3.5 text-center font-semibold">Total Orders</th>
                <th className="px-4 py-3.5 text-center font-semibold">Delivered</th>
                <th className="px-4 py-3.5 text-right font-semibold">Delivery Charges</th>
                <th className="px-4 py-3.5 text-right font-semibold">Sales Handled</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {data.riders.map((r) => (
                <tr key={r.riderKey}>
                  <td className="px-4 py-3.5 font-medium text-fg">{r.riderName}</td>
                  <td className="px-4 py-3.5 text-fg-muted">{r.riderPhone || "—"}</td>
                  <td className="px-4 py-3.5">
                    <span
                      className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase ${
                        r.riderSource === "user"
                          ? "bg-accent-soft text-accent-text"
                          : "bg-surface-2 text-fg-muted"
                      }`}
                    >
                      {r.riderSource === "user" ? "Staff" : "Manual"}
                    </span>
                  </td>
                  <td className="px-4 py-3.5 text-center tabular-nums text-fg">{r.orderCount}</td>
                  <td className="px-4 py-3.5 text-center tabular-nums text-fg">{r.deliveredCount}</td>
                  <td className="px-4 py-3.5 text-right tabular-nums text-fg">
                    {formatMinor(r.deliveryChargesMinor, currency)}
                  </td>
                  <td className="px-4 py-3.5 text-right font-bold tabular-nums text-fg">
                    {formatMinor(r.totalSalesMinor, currency)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Orders List Table */}
      {activeSubTab === "all" && (
        <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
          <div className="border-b border-line px-5 py-4">
            <h3 className="text-sm font-semibold text-fg">Delivery Orders Log</h3>
            <p className="mt-0.5 text-xs text-fg-muted">Every delivery order placed within this reporting period.</p>
          </div>
          <table className="w-full text-left text-sm">
            <thead className="border-b border-line bg-surface-2/50 text-xs font-semibold text-fg-2">
              <tr>
                <th className="px-4 py-3.5 font-semibold">Order #</th>
                <th className="px-4 py-3.5 font-semibold">Type</th>
                <th className="px-4 py-3.5 font-semibold">Status</th>
                <th className="px-4 py-3.5 font-semibold">Rider / Platform</th>
                <th className="px-4 py-3.5 font-semibold">Customer & Phone</th>
                <th className="px-4 py-3.5 font-semibold">Address</th>
                <th className="px-4 py-3.5 text-right font-semibold">Charge</th>
                <th className="px-4 py-3.5 text-right font-semibold">Total</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {data.orders.map((o) => (
                <tr key={o.id}>
                  <td className="px-4 py-3.5 font-medium text-fg">#{o.orderNo}</td>
                  <td className="px-4 py-3">
                    <span
                      className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase ${
                        o.orderType === "delivery"
                          ? "bg-info-soft text-info-text"
                          : "bg-accent-soft text-accent-text"
                      }`}
                    >
                      {o.orderType === "delivery" ? "In-House" : (o.platformName || "3rd-Party")}
                    </span>
                  </td>
                  <td className="px-4 py-3">
                    <span
                      className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-semibold capitalize ${
                        o.deliveryStatus === "delivered"
                          ? "bg-success-soft text-success-text"
                          : o.deliveryStatus === "dispatched"
                            ? "bg-warning-soft text-warning-text"
                            : "bg-surface-2 text-fg-muted"
                      }`}
                    >
                      {o.deliveryStatus || "placed"}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-xs text-fg">
                    {o.riderName ? (
                      <>
                        <span>{o.riderName}</span>
                        {o.riderPhone && <span className="block text-[11px] text-fg-muted">{o.riderPhone}</span>}
                      </>
                    ) : (
                      <span className="text-fg-muted">—</span>
                    )}
                  </td>
                  <td className="px-4 py-3 text-xs text-fg">
                    {o.customerName || o.customerPhone ? (
                      <>
                        <span>{o.customerName || "Customer"}</span>
                        {o.customerPhone && <span className="block text-[11px] text-fg-muted">{o.customerPhone}</span>}
                      </>
                    ) : (
                      <span className="text-fg-muted">—</span>
                    )}
                  </td>
                  <td className="max-w-[200px] truncate px-4 py-3.5 text-xs text-fg-muted" title={o.deliveryAddress || ""}>
                    {o.deliveryAddress || "—"}
                  </td>
                  <td className="px-4 py-3.5 text-right tabular-nums text-xs text-fg-muted">
                    {formatMinor(o.deliveryChargeMinor, currency)}
                  </td>
                  <td className="px-4 py-3.5 text-right font-bold tabular-nums text-fg">
                    {formatMinor(o.totalMinor, currency)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
