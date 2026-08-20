/** Client-side mirror of the Rust-side `images::MAX_IMAGE_BYTES` cap — checked
 * here first so a huge file is rejected instantly instead of after an IPC
 * round trip, but the Rust side re-validates independently. */
export const MAX_IMAGE_BYTES = 5 * 1024 * 1024;

export const ACCEPTED_IMAGE_TYPES = ["image/jpeg", "image/png", "image/webp", "image/gif"];

/** Client-side mirror of `images::MAX_LOGO_BYTES` — smaller than a product
 * photo's cap since a logo is displayed small and uploaded rarely. */
export const MAX_LOGO_BYTES = 2 * 1024 * 1024;

export const ACCEPTED_LOGO_TYPES = ["image/jpeg", "image/png", "image/svg+xml"];

export class ImageTooLargeError extends Error {}
export class UnsupportedImageTypeError extends Error {}

function extensionFor(file: File): string {
  const fromName = file.name.split(".").pop()?.toLowerCase();
  if (fromName) return fromName === "jpeg" ? "jpg" : fromName;
  // "image/svg+xml" -> "svg+xml" if taken naively — strip the "+xml" suffix.
  return file.type.split("/")[1]?.split("+")[0] ?? "png";
}

function readFile(
  file: File,
  acceptedTypes: string[],
  maxBytes: number,
  formatHint: string,
): Promise<{ base64: string; extension: string }> {
  if (!acceptedTypes.includes(file.type)) {
    return Promise.reject(new UnsupportedImageTypeError(`Use ${formatHint}.`));
  }
  if (file.size > maxBytes) {
    return Promise.reject(new ImageTooLargeError(`Image must be under ${maxBytes / 1_048_576} MB.`));
  }

  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("Could not read the image file."));
    reader.onload = () => {
      // reader.result is "data:<mime>;base64,<payload>" — only the payload
      // after the comma is what the backend's base64 decoder expects.
      const result = reader.result as string;
      const base64 = result.slice(result.indexOf(",") + 1);
      resolve({ base64, extension: extensionFor(file) });
    };
    reader.readAsDataURL(file);
  });
}

/**
 * Reads a product-photo `File` into the base64 payload + extension the
 * `inventory_upload_image` command expects. Rejects before any IPC call if
 * the file is too large or not a recognized image type.
 */
export function readImageFile(file: File): Promise<{ base64: string; extension: string }> {
  return readFile(file, ACCEPTED_IMAGE_TYPES, MAX_IMAGE_BYTES, "a JPG, PNG, WEBP or GIF image");
}

/**
 * Same idea, for the business logo (`config_upload_logo`) — a narrower
 * format list (adds SVG, drops WEBP/GIF) and a smaller size cap, matching
 * the Rust side's `images::save_logo`.
 */
export function readLogoFile(file: File): Promise<{ base64: string; extension: string }> {
  return readFile(file, ACCEPTED_LOGO_TYPES, MAX_LOGO_BYTES, "a JPG, PNG or SVG image");
}
