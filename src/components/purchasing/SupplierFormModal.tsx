import { useState, type FormEvent } from "react";
import { addSupplier, updateSupplier } from "../../services/purchasingService";
import type { Supplier } from "../../types";
import { runAction } from "../../utils/runAction";
import { Field, inputClass, ModalCard, primaryButtonClass, secondaryButtonClass } from "../ui/ModalCard";

export function SupplierFormModal({
  supplier,
  onSaved,
  onClose,
}: {
  /** `null` creates a new supplier. */
  supplier: Supplier | null;
  onSaved: () => void;
  onClose: () => void;
}) {
  const [form, setForm] = useState({
    name: supplier?.name ?? "",
    contactPerson: supplier?.contactPerson ?? "",
    phone: supplier?.phone ?? "",
    email: supplier?.email ?? "",
    address: supplier?.address ?? "",
    notes: supplier?.notes ?? "",
  });
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const set = (key: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, [key]: e.target.value });

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    await runAction(setIsSaving, setError, async () => {
      const input = {
        name: form.name.trim(),
        contactPerson: form.contactPerson.trim() || null,
        phone: form.phone.trim() || null,
        email: form.email.trim() || null,
        address: form.address.trim() || null,
        notes: form.notes.trim() || null,
      };
      if (supplier) await updateSupplier(supplier.id, input);
      else await addSupplier(input);
      onSaved();
    });
  };

  return (
    <ModalCard title={supplier ? `Edit ${supplier.name}` : "New supplier"} width="sm" onClose={onClose}>
      <form onSubmit={(e) => void submit(e)} className="space-y-3">
        <Field label="Company name">
          <input value={form.name} onChange={set("name")} autoFocus className={inputClass} />
        </Field>
        <Field label="Contact person">
          <input value={form.contactPerson} onChange={set("contactPerson")} placeholder="Optional" className={inputClass} />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Phone">
            <input value={form.phone} onChange={set("phone")} placeholder="Optional" className={inputClass} />
          </Field>
          <Field label="Email">
            <input value={form.email} onChange={set("email")} placeholder="Optional" className={inputClass} />
          </Field>
        </div>
        <Field label="Address">
          <input value={form.address} onChange={set("address")} placeholder="Optional" className={inputClass} />
        </Field>
        <Field label="Notes">
          <input value={form.notes} onChange={set("notes")} placeholder="Payment terms, delivery days…" className={inputClass} />
        </Field>
        {error && <p className="text-xs text-danger-text">{error}</p>}
        <div className="flex justify-end gap-2 pt-1">
          <button type="button" onClick={onClose} className={secondaryButtonClass}>Cancel</button>
          <button type="submit" disabled={isSaving || !form.name.trim()} className={primaryButtonClass}>
            {isSaving ? "Saving…" : supplier ? "Save changes" : "Add supplier"}
          </button>
        </div>
      </form>
    </ModalCard>
  );
}
