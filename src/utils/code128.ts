/**
 * Code 128 encoder for printed barcode labels. Every retail scanner reads
 * Code 128, and it carries any ASCII text — an existing EAN-13, a
 * supplier's code, or the in-store codes `stock_generate_barcodes` assigns.
 * An all-digit, even-length value uses set C (two digits per symbol, so the
 * label stays short); anything else uses set B.
 */

/** Bar/space module widths per symbol value 0–106 (106 = stop, 7 elements). */
const PATTERNS = [
  "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212", "221213",
  "221312", "231212", "112232", "122132", "122231", "113222", "123122", "123221", "223211", "221132",
  "221231", "213212", "223112", "312131", "311222", "321122", "321221", "312212", "322112", "322211",
  "212123", "212321", "232121", "111323", "131123", "131321", "112313", "132113", "132311", "211313",
  "231113", "231311", "112133", "112331", "132131", "113123", "113321", "133121", "313121", "211331",
  "231131", "213113", "213311", "213131", "311123", "311321", "331121", "312113", "312311", "332111",
  "314111", "221411", "431111", "111224", "111422", "121124", "121421", "141122", "141221", "112214",
  "112412", "122114", "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111",
  "111242", "121142", "121241", "114212", "124112", "124211", "411212", "421112", "421211", "212141",
  "214121", "412121", "111143", "111341", "131141", "114113", "114311", "411113", "411311", "113141",
  "114131", "311141", "411131", "211412", "211214", "211232", "2331112",
];

const START_B = 104;
const START_C = 105;
const STOP = 106;

export { PATTERNS as CODE128_PATTERNS };

/** The symbol values for `text`, including start, checksum and stop. */
export function code128Values(text: string): number[] {
  for (const ch of text) {
    const code = ch.charCodeAt(0);
    if (code < 32 || code > 126) throw new Error(`Can't put "${ch}" in a barcode`);
  }
  const useC = /^\d+$/.test(text) && text.length % 2 === 0 && text.length >= 4;
  const data = useC
    ? Array.from({ length: text.length / 2 }, (_, i) => Number(text.slice(i * 2, i * 2 + 2)))
    : [...text].map((ch) => ch.charCodeAt(0) - 32);
  const start = useC ? START_C : START_B;
  const checksum = data.reduce((sum, value, i) => sum + value * (i + 1), start) % 103;
  return [start, ...data, checksum, STOP];
}

/** Alternating bar/space widths in modules, starting with a bar. */
export function code128Modules(text: string): number[] {
  return code128Values(text).flatMap((value) => [...PATTERNS[value]].map(Number));
}
