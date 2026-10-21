// @ts-expect-error node builtin; the project has no @types/node (tests run in Node)
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// WCAG relative luminance and contrast ratio calculations
function hexToRgb(hex: string): [number, number, number] {
  const cleanHex = hex.replace("#", "");
  const num = parseInt(cleanHex, 16);
  return [(num >> 16) & 255, (num >> 8) & 255, num & 255];
}

function sRgbToLinear(c: number): number {
  const v = c / 255;
  return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
}

function getRelativeLuminance(rgb: [number, number, number]): number {
  const [r, g, b] = rgb.map(sRgbToLinear);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function getContrastRatio(hex1: string, hex2: string): number {
  const lum1 = getRelativeLuminance(hexToRgb(hex1));
  const lum2 = getRelativeLuminance(hexToRgb(hex2));
  const lighter = Math.max(lum1, lum2);
  const darker = Math.min(lum1, lum2);
  return (lighter + 0.05) / (darker + 0.05);
}

function blendRgba(fgHex: string, alpha: number, bgHex: string): string {
  const [fgR, fgG, fgB] = hexToRgb(fgHex);
  const [bgR, bgG, bgB] = hexToRgb(bgHex);
  const r = Math.round(fgR * alpha + bgR * (1 - alpha));
  const g = Math.round(fgG * alpha + bgG * (1 - alpha));
  const b = Math.round(fgB * alpha + bgB * (1 - alpha));
  return `#${((1 << 24) + (r << 16) + (g << 8) + b).toString(16).slice(1)}`;
}

// Read from the real stylesheet so a palette change is re-checked here
// (not `?raw`: Vitest blanks CSS imports).
const css: string = readFileSync(new URL("../styles.css", import.meta.url), "utf8");
const lightBlock = css.slice(css.indexOf(':root[data-theme="light"]'));
function lightVar(name: string): string {
  const m = lightBlock.match(new RegExp(`--t-${name}:\\s*(?:var\\([^,]+,\\s*)?(#[0-9a-fA-F]{6})`));
  if (!m) throw new Error(`--t-${name} not found in the light theme`);
  return m[1];
}

const TOKENS = {
  surfaces: {
    page: lightVar("bg-page"),
    surface: lightVar("bg-surface"),
    sidebar: lightVar("bg-sidebar"),
    surface2: lightVar("surface-2"),
    surface3: lightVar("surface-3"),
    accentTint: lightVar("accent-tint"),
  },
  text: {
    primary: lightVar("text-primary"),
    secondary: lightVar("text-secondary"),
    muted: lightVar("text-muted"),
    accent: lightVar("accent-text"),
  },
  accentSolid: {
    accent: lightVar("accent"),
    ink: lightVar("accent-ink"),
  },
  status: {
    success: { text: lightVar("success-text"), bg: lightVar("success-soft") },
    warning: { text: lightVar("warning-text"), bg: lightVar("warning-soft") },
    danger: { text: lightVar("danger-text"), bg: lightVar("danger-soft") },
    info: { text: lightVar("info-text"), bg: lightVar("info-soft") },
  },
  // Not CSS variables: the darkest pixel of the page gradient and the glass panel's opacity.
  gradientDarkestPixel: "#E5E9E5",
  glassAlpha: 0.85,
};

describe("Light Theme Contrast Verification", () => {
  it("primary text meets or exceeds 7:1 across all light surfaces", () => {
    for (const [name, bg] of Object.entries(TOKENS.surfaces)) {
      const ratio = getContrastRatio(TOKENS.text.primary, bg);
      expect(ratio, `Primary text on ${name} (${bg})`).toBeGreaterThanOrEqual(7.0);
    }
  });

  it("secondary text meets or exceeds 4.5:1 across all light surfaces", () => {
    for (const [name, bg] of Object.entries(TOKENS.surfaces)) {
      const ratio = getContrastRatio(TOKENS.text.secondary, bg);
      expect(ratio, `Secondary text on ${name} (${bg})`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("muted text meets or exceeds 4.5:1 across all light surfaces", () => {
    for (const [name, bg] of Object.entries(TOKENS.surfaces)) {
      const ratio = getContrastRatio(TOKENS.text.muted, bg);
      expect(ratio, `Muted text on ${name} (${bg})`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("accent text meets or exceeds 4.5:1 on surface and tint", () => {
    const ratioSurface = getContrastRatio(TOKENS.text.accent, TOKENS.surfaces.surface);
    const ratioTint = getContrastRatio(TOKENS.text.accent, TOKENS.surfaces.accentTint);
    expect(ratioSurface, "Accent text on surface").toBeGreaterThanOrEqual(4.5);
    expect(ratioTint, "Accent text on accent-tint").toBeGreaterThanOrEqual(4.5);
  });

  it("solid accent button with white ink meets >= 4.5:1 contrast", () => {
    const ratio = getContrastRatio(TOKENS.accentSolid.ink, TOKENS.accentSolid.accent);
    expect(ratio, "Accent button ink").toBeGreaterThanOrEqual(4.5);
  });

  it("all status badge colors meet or exceed 4.5:1 against their tint backgrounds", () => {
    for (const [status, { text, bg }] of Object.entries(TOKENS.status)) {
      const ratio = getContrastRatio(text, bg);
      expect(ratio, `${status} badge text on bg`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("text on glass with worst-case gradient background meets contrast requirements", () => {
    const blendedGlass = blendRgba(
      TOKENS.surfaces.surface,
      TOKENS.glassAlpha,
      TOKENS.gradientDarkestPixel,
    );

    const primaryRatio = getContrastRatio(TOKENS.text.primary, blendedGlass);
    const secondaryRatio = getContrastRatio(TOKENS.text.secondary, blendedGlass);
    const mutedRatio = getContrastRatio(TOKENS.text.muted, blendedGlass);

    expect(primaryRatio, "Primary text on worst-case glass").toBeGreaterThanOrEqual(7.0);
    expect(secondaryRatio, "Secondary text on worst-case glass").toBeGreaterThanOrEqual(4.5);
    expect(mutedRatio, "Muted text on worst-case glass").toBeGreaterThanOrEqual(4.5);
  });
});
