/** Client-side mirror of the Rust-side `images::MAX_IMAGE_BYTES` cap — checked
 * here first so a huge file is rejected instantly instead of after an IPC
 * round trip, but the Rust side re-validates independently. */
export const MAX_IMAGE_BYTES = 5 * 1024 * 1024;

export const ACCEPTED_IMAGE_TYPES = ["image/jpeg", "image/png", "image/webp", "image/gif"];

export class ImageTooLargeError extends Error {}
export class UnsupportedImageTypeError extends Error {}

function extensionFor(file: File): string {
  const fromName = file.name.split(".").pop()?.toLowerCase();
  if (fromName) return fromName === "jpeg" ? "jpg" : fromName;
  return file.type.split("/")[1] ?? "png";
}

/**
 * Reads an image `File` into the base64 payload + extension the
 * `inventory_upload_image` command expects. Rejects before any IPC call if
 * the file is too large or not a recognized image type.
 */
export function readImageFile(file: File): Promise<{ base64: string; extension: string }> {
  if (!ACCEPTED_IMAGE_TYPES.includes(file.type)) {
    return Promise.reject(new UnsupportedImageTypeError("Use a JPG, PNG, WEBP or GIF image."));
  }
  if (file.size > MAX_IMAGE_BYTES) {
    return Promise.reject(new ImageTooLargeError(`Image must be under ${MAX_IMAGE_BYTES / 1_048_576} MB.`));
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
