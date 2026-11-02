import type { ReactNode } from "react";
import { paymentMethodLabel } from "../../utils/checkoutPayments";
import { formatQty } from "../../utils/format";
import { orderTypeLabel, receiptAmount, receiptDate, receiptStamp, riderText } from "../../utils/receiptFormat";
import type { AppConfig, Sale } from "../../types";

interface ReceiptPaperProps {
  sale: Sale;
  config: AppConfig;
  tablesEnabled: boolean;
  logoUrl: string | null;
}

/** The item columns, as on the printed slip: Qty from the left, Rate and
 * Amount flush right, Disc centred (`COLUMNS` in `receiptPdf.ts`). */
const ITEM_GRID = "grid grid-cols-[20fr_26fr_24fr_30fr] items-baseline";

/** The receipt as it prints — the same rows, wording and order as
 * `receiptPdf.ts` and `printer::escpos::receipt_rows` — drawn on white
 * "paper" (`.receipt-paper`) so the screen shows what the customer gets. */
export function ReceiptPaper({ sale, config, tablesEnabled, logoUrl }: ReceiptPaperProps) {
  const stamp = receiptStamp(sale, config);
  const tendered = sale.payments.some((part) => part.method === "cash") ? sale.tenderedMinor ?? null : null;
  const totalQty = sale.items.reduce((sum, line) => sum + line.qty, 0);
  const footer = config.receiptFooter.split("\n").map((line) => line.trim()).filter(Boolean);

  return (
    <div className="receipt-paper mx-auto w-full max-w-[21rem] rounded-sm px-4 py-5 text-[13px] leading-[1.55] shadow-soft">
      {logoUrl && (
        <>
          <img src={logoUrl} alt="" className="mx-auto max-h-24 max-w-[60%] object-contain" />
          <hr className="mx-auto my-2 w-2/3 border-current" />
        </>
      )}
      <p className="text-center text-[17px] font-bold uppercase leading-tight">{config.businessName}</p>
      {[config.businessAddress, config.phone && `Phone No ${config.phone}`, config.deliveryNumber && `Delivery No ${config.deliveryNumber}`, config.businessWebsite]
        .map((line) => line?.trim())
        .filter(Boolean)
        .map((line) => (
          <p key={line} className="text-center">
            {line}
          </p>
        ))}
      {config.receiptTagline?.trim() && (
        <p className="flex items-center gap-2">
          <span className="flex-1 border-t border-dashed border-current" />
          {config.receiptTagline.trim()}
          <span className="flex-1 border-t border-dashed border-current" />
        </p>
      )}

      <div className="mt-2.5">
        <Info label="Cashier:">{(sale.cashierName ?? "—").toUpperCase()}</Info>
        <Info label="Date #:">{receiptDate(sale.createdAt)}</Info>
        <Info label="Invoice #:">{sale.receiptNo}</Info>
        {sale.orderNo && <Info label="Order #:">{sale.orderNo}</Info>}
        {sale.tillName && <Info label="Counter:">{sale.tillName}</Info>}
        <Info label="Type :">{orderTypeLabel(sale, tablesEnabled)}</Info>
        {sale.tableName && <Info label="Table #:">{sale.tableName}</Info>}
        {sale.orderType === "delivery" && (
          <>
            {(sale.customerName || sale.customerPhone) && (
              <Info label="Customer:">{riderText(sale.customerName ?? "", sale.customerPhone).trim()}</Info>
            )}
            {sale.deliveryAddress?.trim() && <Info label="Address:">{sale.deliveryAddress.trim()}</Info>}
            {sale.deliveryRiderName && <Info label="Rider:">{riderText(sale.deliveryRiderName, sale.deliveryRiderPhone)}</Info>}
          </>
        )}
        {sale.orderType === "third_party_delivery" && (
          <>
            {sale.platformOrderRef && <Info label="Order Ref:">#{sale.platformOrderRef}</Info>}
            {sale.deliveryRiderName && <Info label="Rider:">{riderText(sale.deliveryRiderName, sale.deliveryRiderPhone)}</Info>}
          </>
        )}
      </div>

      <div className={`${ITEM_GRID} mt-3 border-b-2 border-current pb-1 font-bold`}>
        <span>Qty</span>
        <span className="text-right">Rate</span>
        <span className="text-center">Disc</span>
        <span className="text-right">Amount</span>
      </div>

      {/* The stamp sits behind the items and totals, like an ink stamp. */}
      <div className="relative isolate">
        {stamp && (
          <span aria-hidden="true" className="pointer-events-none absolute inset-0 -z-10 flex select-none items-center justify-center overflow-hidden">
            <span className="-rotate-[38deg] text-[5.5rem] font-black leading-none tracking-wider opacity-[0.16]">{stamp}</span>
          </span>
        )}
        <ul>
          {sale.items.map((line, i) => (
            <li key={`${line.itemId}-${i}`} className={`py-1 ${i > 0 ? "border-t border-dotted border-current" : ""}`}>
              <p className="font-bold">{line.itemName}</p>
              {line.notes?.trim() && <p className="text-[12px]">({line.notes.trim()})</p>}
              <div className={`${ITEM_GRID} tabular-nums`}>
                <span>{formatQty(line.qty, line.unit)}</span>
                <span className="text-right">{receiptAmount(line.priceAtSaleMinor)}</span>
                <span className="text-center">0</span>
                <span className="text-right">{receiptAmount(line.lineTotalMinor)}</span>
              </div>
            </li>
          ))}
        </ul>

        <dl className="border-y-2 border-current py-1.5">
          <Total label="SubTotal" value={receiptAmount(sale.subtotalMinor)} />
          {sale.discountMinor > 0 && <Total label="Discount" value={`-${receiptAmount(sale.discountMinor)}`} />}
          {sale.taxMinor > 0 && <Total label="Tax" value={receiptAmount(sale.taxMinor)} />}
          {sale.serviceChargeMinor > 0 && <Total label="Service Charge" value={receiptAmount(sale.serviceChargeMinor)} />}
          {!!sale.deliveryChargeMinor && sale.deliveryChargeMinor > 0 && (
            <Total label="Delivery Fee" value={receiptAmount(sale.deliveryChargeMinor)} />
          )}
          <Total label="NetTotal" value={receiptAmount(sale.totalMinor)} />
          {sale.payments.map((part) => (
            <Total
              key={part.method}
              label={`Payment (${paymentMethodLabel(part.method)})`}
              value={receiptAmount(part.method === "cash" && tendered !== null ? tendered : part.amountMinor)}
            />
          ))}
          {sale.dueDate && <Total label="Balance Due By" value={sale.dueDate} />}
          {tendered !== null && (
            <Total label="Change" value={receiptAmount(sale.changeMinor ?? Math.max(0, tendered - sale.totalMinor))} />
          )}
        </dl>
      </div>

      <div className="mt-3 text-center">
        <p className="font-bold">
          Total Items: {sale.items.length} Total QTY: {formatQty(totalQty, null)}
        </p>
        {footer.map((line) => (
          <p key={line}>{line}</p>
        ))}
        {config.receiptSocial?.trim() && <p className="text-[12px]">{config.receiptSocial.trim()}</p>}
        {config.receiptShowCredit && <p className="mt-2 text-[11px]">Developed by Code Hunts™</p>}
      </div>
    </div>
  );
}

function Info({ label, children }: { label: string; children: ReactNode }) {
  return (
    <p className="truncate">
      <span className="font-bold">{label}</span>
      <span className="ml-2">{children}</span>
    </p>
  );
}

function Total({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-3 tabular-nums">
      <dt className="font-bold">{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
