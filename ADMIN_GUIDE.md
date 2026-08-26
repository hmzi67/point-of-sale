# Diwan — Owner's Guide

A simple guide to running your shop with Diwan. No technical knowledge needed.

---

## System requirements

Before installing, make sure the computer meets these minimums:

- **Windows** — Windows 10 or later (64-bit). Windows 7 and 8/8.1 are **not
  supported** — Microsoft stopped updating the underlying browser component
  our app relies on for those versions, so it cannot run there. If you try
  to install on an older Windows machine, the installer will tell you this
  up front instead of failing partway through.
- **macOS** — macOS 10.15 (Catalina) or later.

If a shop's computer is older than this, it will need a Windows 10 (or
newer) machine to run Diwan — there's no way around this requirement, it
comes from Microsoft, not from us.

---

## Getting started

### Signing in

When you open the app you'll see a sign-in screen. Pick your name from the
list and enter your 4–6 digit PIN using the on-screen keypad (or your
keyboard's number keys).

If this is a brand-new installation, sign in as **Owner** with the PIN
**1234**. You'll be asked to set a real PIN during setup — do that first
thing, before anyone else uses the till.

### First-time setup

The very first time you sign in, the app will walk you through a short setup:

1. **Business name** — what shows on receipts and at the top of the app.
2. **What kind of business this is** — Retail shop or Restaurant/café. If
   you pick Restaurant, table management turns on automatically since
   you'll be tracking dine-in orders; a retail shop doesn't need it, so it
   stays off (you can turn it on later if that changes).
3. **Currency** — e.g. PKR, USD, INR. Whatever your prices are in.
4. **Tax rate** — the percentage added to sales, if any. Enter `0` if you
   don't charge tax.

After that you'll land on a screen listing every feature the app can do
(Inventory, Reports, Tables, Attendance, Expenses, Salary, and so on) with a
switch next to each one. Turn on whatever your shop actually uses and leave
the rest off — nothing you turn off is deleted, it's just hidden from the
menu so your staff aren't confused by screens they'll never touch. You can
come back and change any of this later from **Settings**.

### Loading your existing stock

If you already have a list of your products (in Excel or Google Sheets), you
don't have to type them all in by hand:

1. Go to **Inventory** and click **Import CSV**.
2. Click **Download an example CSV** to see the exact format expected.
3. Save your own spreadsheet as a `.csv` file with at least a **name** and a
   **price** column (barcode, category, cost, stock count, and low-stock
   warning level are all optional).
4. Click the upload area in the Import window and pick your file.

The app will import everything it can read and show you a list of any rows
it had to skip (usually a typo in a price, or a blank name) — the good rows
still get added even if a few others need fixing and re-importing.

---

## Daily use

### Adding items to Inventory

1. Go to **Inventory**.
2. Click **Add item**.
3. Fill in the name, price, and how many you have in stock. A barcode,
   category, and photo are optional.
4. Set a **low-stock warning level** — the app will flag the item once stock
   drops to or below that number, so you know to reorder before you run out.
5. Click **Save**.

To change or remove an item later, use the **Edit** and **Delete** buttons
on its row. If an item has ever been part of a sale, deleting it doesn't
erase it completely — it's archived instead, so your old sales records and
reports still make sense.

### Completing a sale (Billing)

This is the screen your cashiers will live in.

1. Type an item's name into the search box, or scan its barcode with a
   barcode scanner (it works like typing — no setup needed).
2. Tap the item to add it to the cart. Tap it again, or adjust the quantity
   field, to add more.
3. If a discount applies, enter it in the **Discount** box (as a flat amount
   or a percentage).
4. Pick a **payment method** (cash, card, or other).
5. If you run a restaurant with table service on, choose the table this sale
   belongs to, or tap **Save to table** to park the order and come back to
   it later (e.g. the table is still eating and you'll bill them when
   they're ready to pay).
6. Click **Complete Sale**. A receipt appears — you can print it (if a
   receipt printer is connected) or download it as a PDF.

Stock levels update automatically the moment a sale completes — you never
need to adjust Inventory by hand after a sale.

**If a table is involved:** when you complete the sale, that table
automatically goes back to "free" and is ready for the next customer.

### Viewing Reports

1. Go to **Reports**.
2. Choose a time range: **Today**, **This Week**, **This Month**, or pick
   your own custom start and end dates.
3. You'll see total sales, number of transactions, your average sale size,
   a day-by-day chart, and your best-selling items.
4. Click **Export PDF** or **Export CSV** to save the report to a file — for
   printing, emailing, or opening in a spreadsheet.

### Tracking attendance (if turned on)

1. Go to **Attendance**.
2. Each staff member has a **Check in** button when their shift starts, which
   turns into **Check out** once they're checked in.
3. The **Attendance log** tab shows a full history you can filter by person
   and date.
4. The **Monthly summary** tab shows how many days each person worked, how
   many they missed, and total hours — this feeds directly into Salary.

### Logging expenses (if turned on)

1. Go to **Expenses**.
2. Fill in the amount, pick or type a category (like "Rent" or "Utilities"),
   the date, and an optional note, then click **Add expense**.
3. The list below shows everything you've logged, filterable by date and
   category, with a running breakdown of where your money's going.

### Paying salaries (if turned on)

1. Go to **Salary**.
2. The **Monthly overview** tab shows what each staff member is owed for the
   month, calculated automatically from their attendance — no manual maths.
3. Click **Record payment** next to someone to log what you actually paid
   them (you can pay in more than one instalment; the app keeps a running
   total and marks them Paid, Partial, or Unpaid).
4. The **Payment history** tab shows every past month for a chosen employee.

### The Dashboard

Your at-a-glance view of how the shop's doing: today's sales, expenses (if
you track them), and net profit, plus a chart of this month's sales trend
and quick links into whatever screens you've turned on. This is the first
screen you see after signing in as an owner or admin.

---

## Managing modules and staff

### Turning features on or off

Go to **Settings**. Every feature has a switch — flip it on or off any time.
Turning something off doesn't delete anything; the data is still there if
you turn it back on later. Billing itself can never be turned off, since
it's the one thing every shop needs.

### Adding, editing, or removing staff accounts

Go to **Users** (Owner/Admin only — cashiers can't see this screen).

- Click **Add staff account** to create a new login: a name, a role
  (Owner, Admin, or Cashier), and a starting PIN.
- Click **Edit** on any account to rename them, change their role, or reset
  their PIN (useful if someone forgets it).
- Click **Deactivate** to disable an account without deleting their history
  — they just won't be able to sign in anymore. You can reactivate them
  later the same way.

**Roles, in plain terms:**
- **Cashier** — can use Billing and look up stock, nothing else. Can't see
  Reports, Expenses, Salary, or Settings, even by typing in a web address.
- **Admin** — can do everything except a couple of Owner-only safety checks
  (only an Owner can create another Owner, or deactivate an Owner account).
- **Owner** — full access, including the two checks above.

You (and the app) can never accidentally lock yourself out — the system
won't let the last remaining Owner account be deactivated.

---

## Troubleshooting

**"Not configured" when I try to print a receipt.**
No thermal printer is connected/set up yet. This doesn't affect the sale at
all — it's already saved. Use the **Download PDF** button on the same
receipt screen instead; that always works and can be printed from any
regular printer.

**A cashier says they can't see Reports/Settings/etc.**
That's expected — cashiers only see Billing and a read-only view of
Inventory. If they need more access, an Owner or Admin can change their role
from the **Users** screen.

**I don't need a feature anymore.**
Turn it off in **Settings** rather than trying to delete data — your history
stays intact and it's back in one click if you change your mind.

**The app was closed unexpectedly (power cut, crash) mid-sale.**
No partial data is ever saved — a sale either completed fully or it didn't
happen at all. Just reopen the app and check Billing; if the sale didn't go
through, the cart may be gone and you'll need to ring it up again, but stock
and totals will never be left half-updated.

---

*Everything in this app works fully offline — no internet connection is
required for any day-to-day task.*
