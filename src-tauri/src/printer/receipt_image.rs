//! The customer receipt drawn as a picture, in a proportional sans-serif
//! (Arimo — Arial's metrics, SIL OFL, `fonts/Arimo-OFL.txt`) instead of the
//! printer's built-in monospace font, which ESC/POS text commands cannot
//! change. The layout (`Row`s) is built in `escpos`; this module only turns
//! it into a 1-bit bitmap and the `GS v 0` raster commands that print it —
//! the same command the logo already uses, so any printer that prints the
//! logo prints this.

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont, VariableFont};

use super::layout::LINE_WIDTH;

const FONT: &[u8] = include_bytes!("../../fonts/Arimo.ttf");

/// Printable width in dots: the measured 42 columns × 12 dots of Font A
/// (see `layout`'s module doc comment) — the real width of the client
/// printers, not the 576-dot spec-sheet figure.
pub const WIDTH_PX: usize = LINE_WIDTH * 12;

/// Rows per `GS v 0` command: small enough for the receive buffer of
/// cheap printers, which can drop one very tall image.
const BAND_ROWS: usize = 128;

/// The data of the largest image command a receipt sends (the logo is smaller).
pub const MAX_IMAGE_BYTES: usize = BAND_ROWS * WIDTH_PX.div_ceil(8);

/// Text sizes in dots, at the head's 203 dpi (~2.47 dots per point): body
/// 10 pt, shop name 13.5 pt, the smallest line 9 pt — nothing prints smaller.
pub const SIZE_BODY: f32 = 25.0;
pub const SIZE_TITLE: f32 = 33.0;
pub const SIZE_SMALL: f32 = 22.0;

/// The stamp: up to ~120 pt, turned 38° counter-clockwise, as a 25% dot
/// screen (like an inked PAID stamp) that stays clear of the text strokes.
const STAMP_MAX_SIZE: f32 = 296.0;
const STAMP_ANGLE_DEG: f32 = 38.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    Thin,
    Thick,
    Dotted,
}

/// One cell of a multi-column row, between two fractions of the width.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub text: String,
    pub from: f32,
    pub to: f32,
    pub align: Align,
    pub bold: bool,
}

impl Cell {
    pub fn new(text: impl Into<String>, from: f32, to: f32, align: Align, bold: bool) -> Self {
        Self { text: text.into(), from, to, align, bold }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    /// Full-width text, word-wrapped onto as many lines as it needs.
    Text { text: String, align: Align, size: f32, bold: bool },
    /// One line of cells; a cell too long for its slot is cut short.
    Cells { cells: Vec<Cell>, size: f32 },
    /// Like `Cells`, but the first cell wraps downward inside its column
    /// (a long item name in the compact design).
    ItemLine { cells: Vec<Cell>, size: f32 },
    /// A bold label with its value right after it: `Date #:  17/10/2026`.
    Pair { label: String, value: String, size: f32 },
    /// Text centred between two dashed runs: `------ text ------`.
    Banner { text: String, size: f32 },
    Rule(Rule),
    /// A thin rule `width` dots wide, centred (the one under the logo).
    ShortRule(usize),
    Gap(usize),
}

impl Row {
    /// Every word on the row, for tests and logs.
    pub fn text(&self) -> String {
        match self {
            Row::Text { text, .. } | Row::Banner { text, .. } => text.clone(),
            Row::Cells { cells, .. } | Row::ItemLine { cells, .. } => cells.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" "),
            Row::Pair { label, value, .. } => format!("{label} {value}"),
            Row::Rule(_) | Row::ShortRule(_) | Row::Gap(_) => String::new(),
        }
    }
}

struct Fonts {
    regular: FontRef<'static>,
    bold: FontRef<'static>,
}

impl Fonts {
    fn load() -> Self {
        let mut regular = FontRef::try_from_slice(FONT).expect("bundled font parses");
        let mut bold = regular.clone();
        // A touch heavier than the font's 400: a thermal head prints thin
        // strokes faintly, and the driver-printed slips this copies read heavy.
        regular.set_variation(b"wght", 500.0);
        bold.set_variation(b"wght", 700.0);
        Self { regular, bold }
    }

    fn pick(&self, bold: bool) -> &FontRef<'static> {
        if bold {
            &self.bold
        } else {
            &self.regular
        }
    }
}

/// An ink-coverage canvas that grows downward as rows are drawn.
struct Canvas {
    width: usize,
    ink: Vec<u8>,
    height: usize,
    /// Where each thick rule was drawn: the stamp sits between the first
    /// (under the item header) and the last (under the totals).
    thick_rules: Vec<usize>,
}

impl Canvas {
    fn new(width: usize) -> Self {
        Self { width, ink: Vec::new(), height: 0, thick_rules: Vec::new() }
    }

    fn grow(&mut self, rows: usize) -> usize {
        let top = self.height;
        self.height += rows;
        self.ink.resize(self.height * self.width, 0);
        top
    }

    fn at(&self, x: i32, y: i32) -> u8 {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            self.ink[y as usize * self.width + x as usize]
        } else {
            0
        }
    }

    fn plot(&mut self, x: i32, y: i32, coverage: u8) {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            let px = &mut self.ink[y as usize * self.width + x as usize];
            *px = (*px).max(coverage);
        }
    }

    fn fill(&mut self, x0: usize, x1: usize, y0: usize, y1: usize) {
        for y in y0..y1 {
            for x in x0..x1.min(self.width) {
                self.plot(x as i32, y as i32, 255);
            }
        }
    }
}

fn line_height(size: f32) -> usize {
    (size * 1.45).ceil() as usize
}

fn text_width(font: &FontRef<'static>, size: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut width = 0.0;
    let mut previous = None;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(prev) = previous {
            width += scaled.kern(prev, id);
        }
        width += scaled.h_advance(id);
        previous = Some(id);
    }
    width
}

/// Cuts `text` to fit `max` px, ending in "…" when anything was dropped.
fn fit(font: &FontRef<'static>, size: f32, text: &str, max: f32) -> String {
    if text_width(font, size, text) <= max {
        return text.to_string();
    }
    let mut cut: String = text.to_string();
    while !cut.is_empty() && text_width(font, size, &format!("{cut}…")) > max {
        cut.pop();
    }
    format!("{}…", cut.trim_end())
}

fn wrap(font: &FontRef<'static>, size: f32, text: &str, max: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        if current.is_empty() || text_width(font, size, &candidate) <= max {
            current = candidate;
        } else {
            lines.push(std::mem::replace(&mut current, word.to_string()));
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines.into_iter().map(|line| fit(font, size, &line, max)).collect()
}

/// Draws `text` with its left edge at `x`, on a line whose top is `top`.
fn draw_text(canvas: &mut Canvas, font: &FontRef<'static>, size: f32, x: f32, top: usize, text: &str) {
    let scale = PxScale::from(size);
    let scaled = font.as_scaled(scale);
    let baseline = top as f32 + (line_height(size) as f32 - (scaled.ascent() - scaled.descent())) / 2.0 + scaled.ascent();
    let mut caret = x;
    let mut previous = None;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(prev) = previous {
            caret += scaled.kern(prev, id);
        }
        if let Some(outline) = font.outline_glyph(id.with_scale_and_position(scale, point(caret, baseline))) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                canvas.plot(bounds.min.x as i32 + gx as i32, bounds.min.y as i32 + gy as i32, (coverage * 255.0) as u8);
            });
        }
        caret += scaled.h_advance(id);
        previous = Some(id);
    }
}

fn place(align: Align, from: f32, to: f32, width: f32) -> f32 {
    match align {
        Align::Left => from,
        Align::Center => from + (to - from - width) / 2.0,
        Align::Right => to - width,
    }
}

/// Side margin, so ink never sits on the printer's unprintable edge.
const MARGIN: f32 = 4.0;

fn draw_row(canvas: &mut Canvas, fonts: &Fonts, row: &Row) {
    let full = (MARGIN, WIDTH_PX as f32 - MARGIN);
    match row {
        Row::Text { text, align, size, bold } => {
            let font = fonts.pick(*bold);
            for line in wrap(font, *size, text, full.1 - full.0) {
                let top = canvas.grow(line_height(*size));
                let x = place(*align, full.0, full.1, text_width(font, *size, &line));
                draw_text(canvas, font, *size, x, top, &line);
            }
        }
        Row::Cells { cells, size } => {
            let top = canvas.grow(line_height(*size));
            let span = full.1 - full.0;
            for cell in cells {
                let (from, to) = (full.0 + cell.from * span, full.0 + cell.to * span);
                let font = fonts.pick(cell.bold);
                let text = fit(font, *size, &cell.text, to - from);
                draw_text(canvas, font, *size, place(cell.align, from, to, text_width(font, *size, &text)), top, &text);
            }
        }
        Row::ItemLine { cells, size } => {
            let span = full.1 - full.0;
            let (name, rest) = cells.split_first().expect("an item line has a name");
            let font = fonts.pick(name.bold);
            let x = full.0 + name.from * span;
            let lines = wrap(font, *size, &name.text, (name.to - name.from) * span);
            let first_top = canvas.height;
            for line in &lines {
                let top = canvas.grow(line_height(*size));
                draw_text(canvas, font, *size, x, top, line);
            }
            for cell in rest {
                let (from, to) = (full.0 + cell.from * span, full.0 + cell.to * span);
                let font = fonts.pick(cell.bold);
                let text = fit(font, *size, &cell.text, to - from);
                draw_text(canvas, font, *size, place(cell.align, from, to, text_width(font, *size, &text)), first_top, &text);
            }
        }
        Row::Banner { text, size } => {
            let font = fonts.pick(false);
            let text = fit(font, *size, text, (full.1 - full.0) * 0.7);
            let width = text_width(font, *size, &text);
            let top = canvas.grow(line_height(*size));
            let x = place(Align::Center, full.0, full.1, width);
            draw_text(canvas, font, *size, x, top, &text);
            let mid = top + line_height(*size) / 2;
            for (start, end) in [(full.0 + 20.0, x - 8.0), (x + width + 8.0, full.1 - 20.0)] {
                let mut dash = start;
                while dash + 6.0 <= end {
                    canvas.fill(dash as usize, (dash + 6.0) as usize, mid, mid + 2);
                    dash += 10.0;
                }
            }
        }
        Row::Rule(rule) => {
            // Light strokes with room around them: a 2-dot line (3 when
            // thick), 7 dots of paper above and below.
            let top = canvas.grow(16);
            if *rule == Rule::Thick {
                canvas.thick_rules.push(top + 8);
            }
            let (x0, x1) = (full.0 as usize, full.1 as usize);
            match rule {
                Rule::Thin => canvas.fill(x0, x1, top + 7, top + 9),
                Rule::Thick => canvas.fill(x0, x1, top + 7, top + 10),
                Rule::Dotted => {
                    for x in (x0..x1).step_by(8) {
                        canvas.fill(x, x + 2, top + 7, top + 9);
                    }
                }
            }
        }
        Row::Pair { label, value, size } => {
            let top = canvas.grow(line_height(*size));
            draw_text(canvas, &fonts.bold, *size, full.0, top, label);
            let x = full.0 + text_width(&fonts.bold, *size, label) + size * 0.6;
            let value = fit(&fonts.regular, *size, value, full.1 - x);
            draw_text(canvas, &fonts.regular, *size, x, top, &value);
        }
        Row::ShortRule(width) => {
            let top = canvas.grow(16);
            let x0 = WIDTH_PX.saturating_sub(*width) / 2;
            canvas.fill(x0, x0 + width, top + 7, top + 9);
        }
        Row::Gap(px) => {
            canvas.grow(*px);
        }
    }
}

/// Lays `text` diagonally over the item and totals area as a dot screen,
/// behind the receipt: no dot lands on or right beside a text stroke.
fn draw_stamp(canvas: &mut Canvas, fonts: &Fonts, text: &str) {
    let (Some(&top), Some(&bottom)) = (canvas.thick_rules.first(), canvas.thick_rules.last()) else { return };
    if bottom <= top {
        return;
    }
    let font = fonts.pick(true);
    let (sin, cos) = STAMP_ANGLE_DEG.to_radians().sin_cos();
    // Width per dot of size, and the glyphs' cap height (~0.72 of the size).
    let per_size = text_width(font, 1.0, text);
    let cap = 0.72;
    let fit_width = WIDTH_PX as f32 / (per_size * cos + cap * sin);
    let fit_height = (bottom - top) as f32 * 1.15 / (per_size * sin + cap * cos);
    let size = STAMP_MAX_SIZE.min(fit_width).min(fit_height);

    let mut glyphs = Canvas::new(text_width(font, size, text).ceil() as usize + 4);
    let line = glyphs.grow(line_height(size));
    draw_text(&mut glyphs, font, size, 2.0, line, text);
    let (half_w, half_h) = (glyphs.width as f32 / 2.0, glyphs.height as f32 / 2.0);
    let (cx, cy) = (WIDTH_PX as f32 / 2.0, (top + bottom) as f32 / 2.0);

    let reach = (half_w * half_w + half_h * half_h).sqrt().ceil() as i32;
    let mut dots = Vec::new();
    for y in (cy as i32 - reach).max(0)..(cy as i32 + reach).min(canvas.height as i32) {
        for x in 0..canvas.width as i32 {
            if x % 4 >= 2 || y % 4 >= 2 {
                continue; // the screen: a 2×2 dot every 4 dots
            }
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            // Back into the upright text's own coordinates.
            let (u, v) = (dx * cos - dy * sin, dx * sin + dy * cos);
            if glyphs.at((u + half_w) as i32, (v + half_h) as i32) < 128 {
                continue;
            }
            let near_text = (-1..=1).any(|oy| (-1..=1).any(|ox| canvas.at(x + ox, y + oy) >= 64));
            if !near_text {
                dots.push((x, y));
            }
        }
    }
    for (x, y) in dots {
        canvas.plot(x, y, 255);
    }
}

/// The rows as a 1-bit bitmap, `WIDTH_PX` wide, packed 8 dots per byte,
/// most significant bit first (ink = 1) — `GS v 0`'s format — with `stamp`
/// (PAID, UNPAID…) behind the item and totals area when given. Returns the
/// bitmap and its height in rows.
pub fn render(rows: &[Row], stamp: Option<&str>) -> (Vec<u8>, usize) {
    let fonts = Fonts::load();
    let mut canvas = Canvas::new(WIDTH_PX);
    for row in rows {
        draw_row(&mut canvas, &fonts, row);
    }
    if let Some(text) = stamp {
        draw_stamp(&mut canvas, &fonts, text);
    }
    let width_bytes = WIDTH_PX.div_ceil(8);
    let mut bitmap = vec![0u8; width_bytes * canvas.height];
    for y in 0..canvas.height {
        for x in 0..WIDTH_PX {
            // Half coverage and up prints: crisp edges on a 1-bit head.
            if canvas.ink[y * WIDTH_PX + x] >= 128 {
                bitmap[y * width_bytes + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    (bitmap, canvas.height)
}

/// `GS v 0` commands printing `bitmap` in bands of `BAND_ROWS`.
pub fn raster_commands(bitmap: &[u8], height: usize) -> Vec<u8> {
    let width_bytes = WIDTH_PX.div_ceil(8);
    let mut out = Vec::with_capacity(bitmap.len() + height.div_ceil(BAND_ROWS) * 8);
    let mut y = 0;
    while y < height {
        let rows = BAND_ROWS.min(height - y);
        out.extend([0x1D, b'v', b'0', 0]);
        out.extend([(width_bytes & 0xFF) as u8, (width_bytes >> 8) as u8, (rows & 0xFF) as u8, (rows >> 8) as u8]);
        out.extend(&bitmap[y * width_bytes..(y + rows) * width_bytes]);
        y += rows;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink_rows(bitmap: &[u8], height: usize) -> usize {
        let width_bytes = WIDTH_PX.div_ceil(8);
        (0..height).filter(|y| bitmap[y * width_bytes..(y + 1) * width_bytes].iter().any(|b| *b != 0)).count()
    }

    #[test]
    fn text_prints_ink_and_bold_prints_more_of_it() {
        let regular = render(&[Row::Text { text: "SubTotal 1,380".into(), align: Align::Left, size: SIZE_BODY, bold: false }], None);
        let bold = render(&[Row::Text { text: "SubTotal 1,380".into(), align: Align::Left, size: SIZE_BODY, bold: true }], None);
        let count = |b: &[u8]| b.iter().map(|byte| byte.count_ones()).sum::<u32>();
        assert!(ink_rows(&regular.0, regular.1) > 10, "the text must actually print");
        assert!(count(&bold.0) > count(&regular.0), "bold must be heavier than regular");
    }

    #[test]
    fn long_text_wraps_onto_more_lines_and_a_long_cell_is_cut_not_spilled() {
        let one = render(&[Row::Text { text: "Rawalpindi".into(), align: Align::Center, size: SIZE_BODY, bold: false }], None).1;
        let long = "5th Road D Block Satellite Town Rawalpindi near the big roundabout opposite the park";
        let wrapped = render(&[Row::Text { text: long.into(), align: Align::Center, size: SIZE_BODY, bold: false }], None).1;
        assert!(wrapped >= 2 * one, "a long address must wrap onto further lines");

        let cell = render(&[Row::Cells { cells: vec![Cell::new(long, 0.0, 0.3, Align::Left, true)], size: SIZE_BODY }], None).1;
        assert_eq!(cell, one, "a cell stays one line high");
    }

    #[test]
    fn the_stamp_sits_behind_the_text_between_the_thick_rules() {
        let rows = vec![
            Row::Text { text: "MASKAN FOOD".into(), align: Align::Center, size: SIZE_TITLE, bold: true },
            Row::Rule(Rule::Thick),
            Row::Text { text: "Half Rotisserie Chicken".into(), align: Align::Left, size: SIZE_BODY, bold: true },
            Row::Gap(200),
            Row::Text { text: "SubTotal 1,380".into(), align: Align::Left, size: SIZE_BODY, bold: true },
            Row::Rule(Rule::Thick),
            Row::Text { text: "Thankyou!!".into(), align: Align::Center, size: SIZE_BODY, bold: false },
        ];
        let (plain, height) = render(&rows, None);
        let (stamped, stamped_height) = render(&rows, Some("PAID"));
        assert_eq!(height, stamped_height);
        assert!(plain.iter().zip(&stamped).all(|(p, s)| p & s == *p), "every text dot is still printed");
        let extra: u32 = plain.iter().zip(&stamped).map(|(p, s)| (s & !p).count_ones()).sum();
        assert!(extra > 1_000, "the stamp must be clearly visible");

        let width_bytes = WIDTH_PX.div_ceil(8);
        let first_rows = 40 * width_bytes; // the shop name, above the first thick rule
        assert_eq!(&plain[..first_rows], &stamped[..first_rows], "nothing stamped outside the item area");
    }

    #[test]
    fn raster_bands_cover_every_row_with_well_formed_headers() {
        let (bitmap, height) = (vec![0xFFu8; WIDTH_PX.div_ceil(8) * 300], 300);
        let out = raster_commands(&bitmap, height);
        let width_bytes = WIDTH_PX.div_ceil(8);
        let mut at = 0;
        let mut rows = 0;
        while at < out.len() {
            assert_eq!(&out[at..at + 4], &[0x1D, b'v', b'0', 0]);
            assert_eq!(out[at + 4] as usize + ((out[at + 5] as usize) << 8), width_bytes);
            let band = out[at + 6] as usize + ((out[at + 7] as usize) << 8);
            assert!(band <= BAND_ROWS);
            rows += band;
            at += 8 + band * width_bytes;
        }
        assert_eq!((rows, at), (height, out.len()));
    }
}
