import { useState, type FormEvent } from "react";
import { addCustomer, updateCustomer } from "../../services/customersService";
import type { Customer } from "../../types";
import { runAction } from "../../utils/runAction";
import { Field, inputClass, ModalCard, primaryButtonClass, secondaryButtonClass } from "../ui/ModalCard";

interface CustomerFormModalProps {
  /** `null` creates a new customer. */
  customer: Customer | null;
  /** Pre-fills the name when creating from a picker search. */
  initialName?: string;
  onSaved: (customer: Customer) => void;
  onClose: () => void;
}

export function CustomerFormModal({ customer, initialName = "", onSaved, onClose }: CustomerFormModalProps) {
  const [name, setName] = useState(customer?.name ?? initialName);
  const [phone, setPhone] = useState(customer?.phone ?? "");
  const [email, setEmail] = useState(customer?.email ?? "");
  const [address, setAddress] = useState(customer?.address ?? "");
  const [notes, setNotes] = useState(customer?.notes ?? "");
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    await runAction(setIsSaving, setError, async () => {
      const input = {
        name: name.trim(),
        phone: phone.trim() || null,
        email: email.trim() || null,
        address: address.trim() || null,
        notes: notes.trim() || null,
      };
      onSaved(customer ? await updateCustomer(customer.id, input) : await addCustomer(input));
    });
  };

  return (
    <ModalCard title={customer ? `Edit ${customer.name}` : "New customer"} width="sm" onClose={onClose}>
      <form onSubmit={(e) => void submit(e)} className="space-y-3">
        <Field label="Name">
          <input value={name} onChange={(e) => setName(e.target.value)} autoFocus className={inputClass} />
        </Field>
        <Field label="Phone">
          <input value={phone} onChange={(e) => setPhone(e.target.value)} inputMode="tel" placeholder="Optional" className={inputClass} />
        </Field>
        <Field label="Email">
          <input value={email} onChange={(e) => setEmail(e.target.value)} type="email" placeholder="Optional" className={inputClass} />
        </Field>
        <Field label="Address">
          <input value={address} onChange={(e) => setAddress(e.target.value)} placeholder="Optional" className={inputClass} />
        </Field>
        <Field label="Notes">
          <input value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="Optional" className={inputClass} />
        </Field>
        {error && <p className="text-xs text-danger-text">{error}</p>}
        <div className="flex justify-end gap-2 pt-1">
          <button type="button" onClick={onClose} className={secondaryButtonClass}>
            Cancel
          </button>
          <button type="submit" disabled={isSaving || !name.trim()} className={primaryButtonClass}>
            {isSaving ? "Saving…" : customer ? "Save changes" : "Add customer"}
          </button>
        </div>
      </form>
    </ModalCard>
  );
}
