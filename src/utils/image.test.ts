import { describe, expect, it } from "vitest";
import { ImageTooLargeError, MAX_IMAGE_BYTES, readImageFile, readLogoFile, UnsupportedImageTypeError } from "./image";

const file = (type: string, size: number, name = "photo.png") => new File([new Uint8Array(size)], name, { type });

describe("image upload checks", () => {
  it("refuses a type the backend won't accept, before reading it", async () => {
    await expect(readImageFile(file("application/pdf", 10, "a.pdf"))).rejects.toBeInstanceOf(UnsupportedImageTypeError);
    await expect(readLogoFile(file("image/gif", 10, "a.gif"))).rejects.toBeInstanceOf(UnsupportedImageTypeError);
  });

  it("refuses a file over the size cap", async () => {
    await expect(readImageFile(file("image/png", MAX_IMAGE_BYTES + 1))).rejects.toBeInstanceOf(ImageTooLargeError);
  });
});
