/** How labels are laid out and what they show — the barcode-label dialog's
 * settings ("barcode customization"). */
export interface LabelOptions {
  /** `roll`: one label per page, for a label printer. `sheet`: a grid of
   * labels on A4 sticker paper. */
  layout: "roll" | "sheet";
  widthMm: number;
  heightMm: number;
  showName: boolean;
  showPrice: boolean;
  showCode: boolean;
  copies: number;
}

export const DEFAULT_LABEL_OPTIONS: LabelOptions = {
  layout: "roll",
  widthMm: 50,
  heightMm: 30,
  showName: true,
  showPrice: true,
  showCode: true,
  copies: 1,
};
