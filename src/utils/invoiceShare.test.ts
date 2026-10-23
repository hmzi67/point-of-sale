import { describe, expect, it } from "vitest";
import { mailtoUrl, whatsappNumber, whatsappUrl } from "./invoiceShare";

describe("whatsappNumber", () => {
  it("turns a local PKR mobile into international form", () => {
    expect(whatsappNumber("0300-1234567", "PKR")).toBe("923001234567");
  });

  it("passes international numbers through as digits", () => {
    expect(whatsappNumber("+44 7700 900123", "PKR")).toBe("447700900123");
    expect(whatsappNumber("0044 7700 900123", "GBP")).toBe("447700900123");
  });

  it("is empty without a phone, letting WhatsApp ask for the contact", () => {
    expect(whatsappUrl(null, "PKR", "hi")).toBe("https://wa.me/?text=hi");
  });
});

describe("mailtoUrl", () => {
  it("encodes subject and body", () => {
    expect(mailtoUrl("a@b.com", "Invoice #1", "Total: 10")).toBe("mailto:a%40b.com?subject=Invoice%20%231&body=Total%3A%2010");
  });
});
