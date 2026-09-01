//! Software renderer: draws the menu into an ARGB8888 (BGRA byte order) shm buffer.
//! Pure CPU, no Wayland dependency, so it is unit-testable headless.

use ab_glyph::{point, Font, Glyph, GlyphId, PxScale, ScaleFont};

pub type Bgra = [u8; 4];

pub const fn bgra(r: u8, g: u8, b: u8, a: u8) -> Bgra {
    [b, g, r, a]
}

/// Spotlight panel corner radius (logical px), clamped to half the panel size.
pub const CORNER_RADIUS: u32 = 10;

pub const BG_NORMAL: Bgra = bgra(0x22, 0x22, 0x22, 0xff);
pub const FG_NORMAL: Bgra = bgra(0xbb, 0xbb, 0xbb, 0xff);
/// Input bar: a subtly lighter tone than the list, so bar and list read as
/// separate pieces of one panel.
pub const BG_PROMPT: Bgra = bgra(0x2e, 0x2e, 0x2e, 0xff);
pub const FG_PROMPT: Bgra = bgra(0xee, 0xee, 0xee, 0xff);
/// Prompt label strip: sits only behind the prompt text (e.g. "address"), so
/// the label reads apart from the input content area (which stays `bg_prompt`).
pub const BG_LABEL: Bgra = bgra(0x00, 0x45, 0x60, 0xff);
pub const BG_SEL: Bgra = bgra(0x00, 0x55, 0x77, 0xff);
pub const FG_SEL: Bgra = bgra(0xee, 0xee, 0xee, 0xff);

/// Menu color scheme, wmenu-style (defaults match the old hardcoded constants).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
    /// Normal row background (`-N`).
    pub bg_normal: Bgra,
    /// Normal row foreground (`-n`).
    pub fg_normal: Bgra,
    /// Prompt/input row background (`-M`).
    pub bg_prompt: Bgra,
    /// Prompt label strip background (input bar stays `bg_prompt`).
    pub bg_label: Bgra,
    /// Prompt/input row foreground (`-m`).
    pub fg_prompt: Bgra,
    /// Selected row background (`-S`).
    pub bg_sel: Bgra,
    /// Selected row foreground (`-s`).
    pub fg_sel: Bgra,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            bg_normal: BG_NORMAL,
            fg_normal: FG_NORMAL,
            bg_prompt: BG_PROMPT,
            bg_label: BG_LABEL,
            fg_prompt: FG_PROMPT,
            bg_sel: BG_SEL,
            fg_sel: FG_SEL,
        }
    }
}

/// Parse a wmenu `RRGGBB` or `RRGGBBAA` color into BGRA `[b, g, r, a]`.
pub fn parse_color(s: &str) -> Option<Bgra> {
    let s = s.strip_prefix('#').unwrap_or(s);
    let v = u32::from_str_radix(s, 16).ok()?;
    match s.len() {
        6 => Some([(v & 0xff) as u8, ((v >> 8) & 0xff) as u8, ((v >> 16) & 0xff) as u8, 0xff]),
        8 => Some([((v >> 8) & 0xff) as u8, ((v >> 16) & 0xff) as u8, ((v >> 24) & 0xff) as u8, (v & 0xff) as u8]),
        _ => None,
    }
}

/// One visible item row (text already resolved).
pub struct Row<'a> {
    pub text: &'a str,
    pub selected: bool,
}

/// Horizontal pixel span `[x0, x1)` of the rounded panel at row `y`
/// (x0 inclusive, x1 exclusive). Corner pixels fall outside the span;
/// middle rows span the full width. Radius is clamped to half a dimension.
pub fn rounded_span(w: u32, h: u32, r: u32, y: u32) -> (u32, u32) {
    let r = r.min(w / 2).min(h / 2);
    if r == 0 {
        return if y < h { (0, w) } else { (0, 0) };
    }
    let rr = (r * r) as f64;
    let py = y as f64 + 0.5;
    let top = r as f64;
    let bottom = h as f64 - r as f64;
    if py >= top && py <= bottom {
        return (0, w);
    }
    let dy = if py < top { top - py } else { py - bottom };
    let inset = (r as f64 - (rr - dy * dy).max(0.0).sqrt()).ceil() as u32;
    (inset.min(w), w.saturating_sub(inset))
}

fn set_span(buf: &mut [u8], w: u32, y: u32, x0: u32, x1: u32, color: Bgra) {
    let row = &mut buf[(y * w + x0) as usize * 4..(y * w + x1) as usize * 4];
    for px in row.chunks_exact_mut(4) {
        px.copy_from_slice(&color);
    }
}

/// Draws the full frame. `buf` is `w*h*4` bytes in BGRA order.
/// The first row is the prompt/input row, the rest are `rows`.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    buf: &mut [u8],
    w: u32,
    h: u32,
    font: &crate::font::MenuFont,
    prompt: &str,
    query: &str,
    password: bool,
    rows: &[Row],
    padding: u32,
    colors: &Colors,
) {
    let row_h = font.row_h;
    let p = padding;
    // `-P` masks the typed text; filtering still sees the real query.
    let query = masked_input(query, password);

    // Panel: rounded corners (transparent outside), input bar and list sharing
    // one outline, separated only by the subtle background difference.
    let r = CORNER_RADIUS.min(h / 2);
    fill(buf, w, h, [0, 0, 0, 0]);
    for y in 0..h {
        let (x0, x1) = rounded_span(w, h, r, y);
        let bg = if y < row_h { colors.bg_prompt } else { colors.bg_normal };
        set_span(buf, w, y, x0, x1, bg);
    }

    // Prompt / input row text.
    let baseline = row_baseline(font, 0);
    // The prompt label ("address") gets a distinct background strip, clipped to
    // the rounded outline like the selection band below.
    let mut x = p as f32;
    if !prompt.is_empty() {
        let label_end = (p + measure(font, prompt, (w - 2 * p) as f32) as u32).min(w);
        for yy in 0..row_h.min(h) {
            let (x0, x1) = rounded_span(w, h, r, yy);
            set_span(buf, w, yy, x0, x1.min(label_end), colors.bg_label);
        }
        x = draw_text(buf, w, h, font, p as f32, baseline, prompt, colors.fg_prompt, (w - 2 * p) as f32);
    }
    if !query.is_empty() {
        x = draw_text(buf, w, h, font, x, baseline, &query, colors.fg_prompt, w as f32 - x - p as f32);
    }
    // caret: 2px bar after the input text, spanning the text's ascent→descent
    // block (descent_px is negative in ab_glyph, so text bottom = baseline - descent).
    let caret_x = x as u32 + 1;
    let caret_bottom = (baseline - font.descent_px).round() as u32;
    let caret_h = font.line_h.ceil() as u32;
    if caret_bottom > 0 {
        let caret_top = caret_bottom.saturating_sub(caret_h);
        rect(buf, w, h, caret_x.min(w - 2), caret_top, 2, caret_bottom - caret_top, colors.fg_prompt);
    }

    // Item rows (background already bg_normal from the panel pass; only the
    // selected strip needs an override, clipped to the rounded outline).
    for (i, row) in rows.iter().enumerate() {
        let y = row_h * (i as u32 + 1);
        if row.selected {
            for yy in y..(y + row_h).min(h) {
                let (x0, x1) = rounded_span(w, h, r, yy);
                set_span(buf, w, yy, x0, x1, colors.bg_sel);
            }
        }
        let (_, fg) = if row.selected { (colors.bg_sel, colors.fg_sel) } else { (colors.bg_normal, colors.fg_normal) };
        draw_text(buf, w, h, font, p as f32, row_baseline(font, y), row.text, fg, w as f32 - 2.0 * p as f32);
    }
}

pub fn fill(buf: &mut [u8], w: u32, h: u32, color: Bgra) {
    for px in buf[..(w * h * 4) as usize].chunks_exact_mut(4) {
        px.copy_from_slice(&color);
    }
}

pub fn rect(buf: &mut [u8], w: u32, h: u32, x: u32, y: u32, rw: u32, rh: u32, color: Bgra) {
    let x0 = (x as i64).clamp(0, w as i64) as u32;
    let y0 = (y as i64).clamp(0, h as i64) as u32;
    let x1 = ((x + rw) as i64).clamp(0, w as i64) as u32;
    let y1 = ((y + rh) as i64).clamp(0, h as i64) as u32;
    for py in y0..y1 {
        let row = &mut buf[(py * w + x0) as usize * 4..(py * w + x1) as usize * 4];
        for px in row.chunks_exact_mut(4) {
            px.copy_from_slice(&color);
        }
    }
}

/// Draw `s` starting at (x, baseline), clipped to `max_w` px wide.
/// Returns the x position just past the last drawn advance.
pub fn draw_text(
    buf: &mut [u8],
    w: u32,
    h: u32,
    font: &crate::font::MenuFont,
    x: f32,
    baseline: f32,
    s: &str,
    color: Bgra,
    max_w: f32,
) -> f32 {
    let scale = || PxScale::from(font.size);
    let mut cx = x;
    let end = x + max_w;
    'ch: for ch in s.chars() {
        // Walk the face chain (primary first); draw with the first face that
        // has this glyph so CJK etc. fall back to a system font.
        for face in std::iter::once(&font.font).chain(font.fallbacks.iter()) {
            let scaled = face.as_scaled(scale());
            let gid = scaled.glyph_id(ch);
            if gid == GlyphId(0) {
                continue; // no glyph in this face, try the next
            }
            let adv = scaled.h_advance(gid);
            if cx + adv > end {
                break 'ch;
            }
            if let Some(outline) =
                face.outline_glyph(Glyph { id: gid, scale: scale(), position: point(cx, baseline) })
            {
                let b = outline.px_bounds();
                outline.draw(|gx, gy, cov| {
                    let px = b.min.x as i32 + gx as i32;
                    let py = b.min.y as i32 + gy as i32;
                    if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                        return;
                    }
                    let a = (cov * color[3] as f32).round() as u32;
                    if a == 0 {
                        return;
                    }
                    blend_px(
                        &mut buf[((py as u32 * w + px as u32) * 4) as usize..][..4],
                        [color[0], color[1], color[2]],
                        a,
                    );
                });
            }
            cx += adv;
            continue 'ch;
        }
        // No face in the chain has this glyph; skip it silently.
    }
    cx
}

/// Advance width of `s` under the same face-chain + fit rules as `draw_text`
/// (returns the extent starting at 0, capped at `max_w`). Kept in lockstep
/// with `draw_text`'s per-glyph fit break.
fn measure(font: &crate::font::MenuFont, s: &str, max_w: f32) -> f32 {
    let scale = || PxScale::from(font.size);
    let mut cx = 0.0;
    'ch: for ch in s.chars() {
        for face in std::iter::once(&font.font).chain(font.fallbacks.iter()) {
            let scaled = face.as_scaled(scale());
            let gid = scaled.glyph_id(ch);
            if gid == GlyphId(0) {
                continue;
            }
            let adv = scaled.h_advance(gid);
            if cx + adv > max_w {
                break 'ch;
            }
            cx += adv;
            continue 'ch;
        }
    }
    cx
}

fn row_baseline(font: &crate::font::MenuFont, row_y: u32) -> f32 {
    let top = row_y as f32 + (font.row_h as f32 - font.line_h) / 2.0;
    top + font.ascent_px
}

/// `-P` password mode: one asterisk per char; otherwise return the query unchanged.
fn masked_input(query: &str, password: bool) -> String {
    if password { "*".repeat(query.chars().count()) } else { query.to_string() }
}

#[inline]
fn blend_px(dst: &mut [u8], src: [u8; 3], a: u32) {
    let inv = 255 - a;
    dst[0] = ((src[0] as u32 * a + dst[0] as u32 * inv) / 255) as u8;
    dst[1] = ((src[1] as u32 * a + dst[1] as u32 * inv) / 255) as u8;
    dst[2] = ((src[2] as u32 * a + dst[2] as u32 * inv) / 255) as u8;
    // dst alpha stays 255 (fully opaque menu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::MenuFont;

    /// Headless render check: draws a real frame (text incl. CJK) to a raw BGRA dump.
    #[test]
    fn renders_frame_with_cjk_text() {
        let font = MenuFont::load(None, 16.0).expect("system font available");
        let (w, h) = (320u32, font.row_h * 4);
        let mut buf = vec![0u8; (w * h * 4) as usize];

        let colors = Colors::default();
        let rows = [
            Row { text: "Firefox 火狐浏览器", selected: true },
            Row { text: "Terminal", selected: false },
            Row { text: "Settings", selected: false },
        ];
        draw(&mut buf, w, h, &font, "run: ", "fir", false, &rows, crate::PAD, &colors);

        // Selected row (rows[0]) has the selection background.
        let sel_px = &buf[(font.row_h * 1 * w + w / 2) as usize * 4..][..4];
        assert_eq!(sel_px, &colors.bg_sel);
        // Normal row (rows[1]) has the normal background.
        let norm_px = &buf[(font.row_h * 2 * w + w / 2) as usize * 4..][..4];
        assert_eq!(norm_px, &colors.bg_normal);
        // Prompt row has the prompt background.
        let prompt_px = &buf[(w / 2) as usize * 4..][..4];
        assert_eq!(prompt_px, &colors.bg_prompt);
        // First text row must contain non-background pixels (glyph ink).
        let mut text_has_ink = false;
        'outer: for y in 2..24u32 {
            for x in 2..200u32 {
                let i = ((font.row_h + y) * w + x) as usize * 4;
                if [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]] != colors.bg_sel {
                    text_has_ink = true;
                    break 'outer;
                }
            }
        }
        assert!(text_has_ink, "text should be drawn over the selection row");

        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/frame-demo.bgra");
        std::fs::write(&out, &buf).expect("raw frame written");
        assert!(out.exists() && out.metadata().unwrap().len() > 1000);
    }

    /// Render check: latin-only primary face, CJK must still draw via fallback.
    #[test]
    fn renders_cjk_via_fallback_when_primary_is_latin() {
        let font = MenuFont::load(Some("Noto Sans Mono"), 16.0).expect("system font available");
        let (w, h) = (320u32, font.row_h);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        draw(&mut buf, w, h, &font, "", "火狐", false, &[], crate::PAD, &Colors::default());
        // Prompt row bg is BG_PROMPT; any pixel differing from it is glyph ink.
        let bg = Colors::default().bg_prompt;
        let mut ink = false;
        'o: for y in 2..h - 2 {
            for x in 2..w - 2 {
                let i = (y * w + x) as usize * 4;
                if [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]] != bg {
                    ink = true;
                    break 'o;
                }
            }
        }
        assert!(ink, "CJK glyphs must render via the fallback chain");
    }

    #[test]
    fn rounded_corner_pixels_are_transparent_inside_is_opaque() {
        let font = MenuFont::load(None, 16.0).expect("system font available");
        let (w, h) = (200u32, font.row_h * 3);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let colors = Colors::default();
        draw(&mut buf, w, h, &font, "", "fir", false, &[], crate::PAD, &colors);
        let px = |x: u32, y: u32| &buf[((y * w + x) * 4) as usize..][..4];

        // All four corner pixels are fully transparent.
        assert_eq!(px(0, 0), &[0, 0, 0, 0]);
        assert_eq!(px(w - 1, 0), &[0, 0, 0, 0]);
        assert_eq!(px(0, h - 1), &[0, 0, 0, 0]);
        assert_eq!(px(w - 1, h - 1), &[0, 0, 0, 0]);
        // The interior near a corner is opaque and opaque panel-colored.
        assert_eq!(px(6, 2)[3], 0xff);
        assert_eq!(px(w / 2, h / 2), &colors.bg_normal);
    }

    #[test]
    fn rounded_span_outline_geometry() {
        let (w, h, r) = (200u32, 40u32, 10u32);
        // Top corner row starts inset from the left edge (curve), shrinks to full width.
        let (x0, _) = rounded_span(w, h, r, 0);
        assert!(x0 > 0 && x0 < r);
        // Middle rows are full width.
        assert_eq!(rounded_span(w, h, r, 20), (0, w));
        // Bottom corner row ends before the right edge.
        let (_, x1) = rounded_span(w, h, r, h - 1);
        assert!(x1 > w - r && x1 < w);
        // Zero radius = full rect.
        assert_eq!(rounded_span(w, h, 0, 0), (0, w));
        // Radius clamped to half the height: nearly full-width spans, not deep corners.
        let (x0, x1) = rounded_span(w, 2, 10, 0);
        assert!(x0 <= 1 && x1 >= w - 1);
    }

    #[test]
    fn prompt_bar_and_list_have_distinct_backgrounds() {
        let def = Colors::default();
        assert_ne!(def.bg_prompt, def.bg_normal, "input bar color must differ from list color");

        let font = MenuFont::load(None, 16.0).expect("system font available");
        let (w, h) = (200u32, font.row_h * 2);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        draw(&mut buf, w, h, &font, "", "x", false, &[Row { text: "app", selected: false }], crate::PAD, &def);
        let px = |x: u32, y: u32| &buf[((y * w + x) * 4) as usize..][..4];
        // Input bar row uses bg_prompt, the list row below it bg_normal.
        assert_eq!(px(w / 2, 2), &def.bg_prompt);
        assert_eq!(px(w / 2, font.row_h + 2), &def.bg_normal);
    }

    #[test]
    fn prompt_label_has_distinct_background_from_input_area() {
        let def = Colors::default();
        assert_ne!(def.bg_label, def.bg_prompt, "label strip must differ from input area");

        let font = MenuFont::load(None, 16.0).expect("system font available");
        let (w, h) = (240u32, font.row_h);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        draw(&mut buf, w, h, &font, "address", "", false, &[], crate::PAD, &def);
        let mid = font.row_h / 2;
        let px = |x: u32| &buf[((mid * w + x) * 4) as usize..][..4];
        // Behind the prompt text: label strip; right of it: input bar color.
        assert_eq!(px(crate::PAD), &def.bg_label);
        let label_end = crate::PAD + measure(&font, "address", (w - 2 * crate::PAD) as f32) as u32;
        // +8 clears any antialiased bleed past the last glyph's advance.
        assert_eq!(px(label_end + 8), &def.bg_prompt);

        // Empty prompt → no strip; the whole bar stays bg_prompt.
        let mut buf2 = vec![0u8; (w * h * 4) as usize];
        draw(&mut buf2, w, h, &font, "", "x", false, &[], crate::PAD, &def);
        assert_eq!(&buf2[((mid * w + crate::PAD) * 4) as usize..][..4], &def.bg_prompt);
    }

    #[test]
    fn password_mode_masks_query_with_asterisks() {
        assert_eq!(masked_input("hunter2", true), "*******");
        assert_eq!(masked_input("中文", true), "**");
        assert_eq!(masked_input("hunter2", false), "hunter2");
    }

    #[test]
    fn spacing_follows_eye_comfort_rules() {
        let font = MenuFont::load(None, 16.0).expect("system font available");
        // Row height ≈1.5× the font: glyphs don't crowd, the selection band is a
        // comfortable target. (line_h == font size by PxScale definition.)
        assert!(
            font.row_h as f32 >= font.line_h * 1.4,
            "row {} should be >= 1.4x line {}",
            font.row_h,
            font.line_h
        );
        // Content inset must clear the corner radius so text never grazes the curve.
        // (Constants: these are compile-time guards for the comfort rules.)
        #[allow(clippy::assertions_on_constants)]
        assert!(crate::PAD >= CORNER_RADIUS, "PAD {} < radius {}", crate::PAD, CORNER_RADIUS);
        // Away from the screen edge: top margin is a comfortable float, not a hug.
        #[allow(clippy::assertions_on_constants)]
        assert!(crate::TOP_MARGIN >= 32, "top margin too tight");
    }

    #[test]
    fn parses_wmenu_colors() {
        // RRGGBB → BGRA [b, g, r, 0xff]
        assert_eq!(parse_color("bb2222"), Some([0x22, 0x22, 0xbb, 0xff]));
        assert_eq!(parse_color("005577"), Some([0x77, 0x55, 0x00, 0xff]));
        // RRGGBBAA → [b, g, r, a]
        assert_eq!(parse_color("11223344"), Some([0x33, 0x22, 0x11, 0x44]));
        assert_eq!(parse_color("12345"), None);
        assert_eq!(parse_color("zzzzzz"), None);
        // wmenu-style '#' prefix is accepted; malformed inputs still rejected.
        assert_eq!(parse_color("#bb2222"), Some([0x22, 0x22, 0xbb, 0xff]));
        assert_eq!(parse_color("#11223344"), Some([0x33, 0x22, 0x11, 0x44]));
        assert_eq!(parse_color("##123456"), None);
        assert_eq!(parse_color("#12345"), None);
    }
// caret geometry: solid 2px bar right after the query text, spanning the
// text's ascent→descent block (not hidden by row height, not shifted by the
// negative descent that ab_glyph reports).
#[cfg(test)]
mod caret_tests {
    use super::*;
    use crate::font::MenuFont;

    #[test]
    fn caret_is_solid_bar_just_past_query_spanning_text_block() {
        let font = MenuFont::load(None, 16.0).expect("system font available");
        let (w, h) = (200u32, font.row_h);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let colors = Colors::default();
        draw(&mut buf, w, h, &font, "", "fir", false, &[], crate::PAD, &colors);
        let px = |x: u32, y: u32| &buf[((y * w + x) * 4) as usize..][..4];

        // A solid (non-antialiased) 2px-wide fg_prompt column = the caret;
        // glyph ink is always anti-aliased, so it can't fake this.
        let mid = font.row_h / 2;
        let mut caret_x = None;
        for x in 1..w - 3 {
            if px(x, mid) == &colors.fg_prompt && px(x + 1, mid) == &colors.fg_prompt
                && px(x - 1, mid) != &colors.fg_prompt
                && px(x + 2, mid) != &colors.fg_prompt
            {
                caret_x = Some(x);
                break;
            }
        }
        let x = caret_x.expect("caret must be drawn after the query text");
        // Caret sits just past the text (starts at the panel padding).
        assert!(x > crate::PAD + 4, "caret at x={x} should follow the text");
        // Vertical span == the line block centered in the (taller) row.
        let line_h = font.line_h.ceil() as u32;
        let top_expected = (font.row_h - line_h) / 2;
        let mut top = None;
        let mut bottom = None;
        for y in 0..font.row_h {
            let solid = px(x, y) == &colors.fg_prompt && px(x + 1, y) == &colors.fg_prompt;
            if solid && top.is_none() { top = Some(y); }
            if solid { bottom = Some(y); }
        }
        assert_eq!(top, Some(top_expected), "caret must start at the text block top");
        assert_eq!(bottom, Some(top_expected + line_h - 1), "caret must span the full text block");
    }
}

}
