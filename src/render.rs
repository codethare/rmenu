//! Software renderer: draws the menu into an ARGB8888 (BGRA byte order) shm buffer.
//! Pure CPU, no Wayland dependency, so it is unit-testable headless.

use ab_glyph::{point, Font, Glyph, PxScale, ScaleFont};
use image::{imageops, RgbaImage};

pub type Bgra = [u8; 4];

pub const fn bgra(r: u8, g: u8, b: u8, a: u8) -> Bgra {
    [b, g, r, a]
}

pub const BG_NORMAL: Bgra = bgra(0x22, 0x22, 0x22, 0xff);
pub const FG_NORMAL: Bgra = bgra(0xbb, 0xbb, 0xbb, 0xff);
pub const BG_PROMPT: Bgra = bgra(0x00, 0x55, 0x77, 0xff);
pub const FG_PROMPT: Bgra = bgra(0xee, 0xee, 0xee, 0xff);
pub const BG_SEL: Bgra = bgra(0x00, 0x55, 0x77, 0xff);
pub const FG_SEL: Bgra = bgra(0xee, 0xee, 0xee, 0xff);

/// One visible item row (icon and text already resolved).
pub struct Row<'a> {
    pub icon: Option<&'a RgbaImage>,
    pub text: &'a str,
    pub selected: bool,
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
    rows: &[Row],
    padding: u32,
) {
    fill(buf, w, h, BG_NORMAL);

    let row_h = font.row_h;
    let p = padding;
    let icon_sz = row_h - 2 * p;
    let x_icon_end = p + icon_sz + p;

    // Prompt / input row.
    rect(buf, w, h, 0, 0, w, row_h, BG_PROMPT);
    let baseline = row_baseline(font, 0);
    let mut x = draw_text(buf, w, h, font, p as f32, baseline, prompt, FG_PROMPT, (w - 2 * p) as f32);
    if !query.is_empty() {
        x = draw_text(buf, w, h, font, x, baseline, query, FG_PROMPT, w as f32 - x - p as f32);
    }
    // caret: 2px bar after the input text
    let caret_x = x as u32 + 1;
    let caret_top = (baseline + font.descent_px).round() as u32;
    let caret_h = font.line_h.ceil() as u32;
    if caret_h > 0 && caret_top >= caret_h {
        rect(buf, w, h, caret_x.min(w - 2), caret_top - caret_h, 2, caret_h, FG_PROMPT);
    }

    // Item rows.
    for (i, row) in rows.iter().enumerate() {
        let y = row_h * (i as u32 + 1);
        let (bg, fg) = if row.selected { (BG_SEL, FG_SEL) } else { (BG_NORMAL, FG_NORMAL) };
        rect(buf, w, h, 0, y, w, row_h, bg);
        let mut x = p as f32;
        if let Some(icon) = row.icon {
            blit_scaled(buf, w, h, icon, p, y + p, icon_sz);
            x = x_icon_end as f32;
        }
        draw_text(buf, w, h, font, x, row_baseline(font, y), row.text, fg, w as f32 - x - p as f32);
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

/// Straight-alpha blend `img` at (x, y), clipping to the buffer.
pub fn blit(buf: &mut [u8], w: u32, h: u32, img: &RgbaImage, x: i32, y: i32) {
    let iw = img.width() as usize;
    for (i, px) in img.pixels().enumerate() {
        let px_x = x + (i % iw) as i32;
        let py = y + (i / iw) as i32;
        if px_x < 0 || py < 0 || px_x >= w as i32 || py >= h as i32 {
            continue;
        }
        let s = px.0;
        let a = s[3] as u32;
        if a == 0 {
            continue;
        }
        blend_px(
            &mut buf[(py as u32 * w + px_x as u32) as usize * 4..][..4],
            [s[0], s[1], s[2]],
            a,
        );
    }
}

/// Scale `img` (keeping aspect, bounding box `target`) and blit centered in the box.
pub fn blit_scaled(buf: &mut [u8], w: u32, h: u32, img: &RgbaImage, x: u32, y: u32, target: u32) {
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 || target == 0 {
        return;
    }
    let f = target as f32 / iw.max(ih) as f32;
    let nw = ((iw as f32 * f).round().max(1.0)) as u32;
    let nh = ((ih as f32 * f).round().max(1.0)) as u32;
    let scaled = if nw == iw && nh == ih {
        img.clone()
    } else {
        imageops::resize(img, nw, nh, imageops::FilterType::Triangle)
    };
    let ox = x + (target - nw) / 2;
    let oy = y + (target - nh) / 2;
    blit(buf, w, h, &scaled, ox as i32, oy as i32);
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
    let scaled = font.font.as_scaled(PxScale::from(font.size));
    let mut cx = x;
    let end = x + max_w;
    for ch in s.chars() {
        let gid = scaled.glyph_id(ch);
        let adv = scaled.h_advance(gid);
        if cx + adv > end {
            break;
        }
        if let Some(outline) = font
            .font
            .outline_glyph(Glyph { id: gid, scale: PxScale::from(font.size), position: point(cx, baseline) })
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
    }
    cx
}

fn row_baseline(font: &crate::font::MenuFont, row_y: u32) -> f32 {
    let top = row_y as f32 + (font.row_h as f32 - font.line_h) / 2.0;
    top + font.ascent_px
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

    /// Headless render check: draws a real frame (icon + CJK text) to a PNG.
    #[test]
    fn renders_frame_with_icon_and_cjk() {
        let font = MenuFont::load(None, 16.0).expect("system font available");
        let (w, h) = (320u32, font.row_h * 4);
        let mut buf = vec![0u8; (w * h * 4) as usize];

        let icon = crate::icon::load("fcitx-wbpy", 32);
        let rows = [
            Row { icon: icon.as_ref(), text: "Firefox 火狐浏览器", selected: true },
            Row { icon: None, text: "Terminal", selected: false },
            Row { icon: None, text: "Settings", selected: false },
        ];
        draw(&mut buf, w, h, &font, "run: ", "fir", &rows, 4);

        // Selected row (rows[0]) has the selection background.
        let sel_px = &buf[(font.row_h * 1 * w + w / 2) as usize * 4..][..4];
        assert_eq!(sel_px, &BG_SEL);
        // Normal row (rows[1]) has the normal background.
        let norm_px = &buf[(font.row_h * 2 * w + w / 2) as usize * 4..][..4];
        assert_eq!(norm_px, &BG_NORMAL);
        // Prompt row has the prompt background.
        let prompt_px = &buf[(w / 2) as usize * 4..][..4];
        assert_eq!(prompt_px, &BG_PROMPT);
        // Icon box (top-left of the selected row) must contain non-background pixels.
        let mut icon_has_ink = false;
        'outer: for y in 2..24u32 {
            for x in 2..24u32 {
                let i = ((font.row_h + y) * w + x) as usize * 4;
                if [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]] != BG_SEL {
                    icon_has_ink = true;
                    break 'outer;
                }
            }
        }
        assert!(icon_has_ink, "icon should be drawn over the selection row");

        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/frame-demo.png");
        image::save_buffer(&out, &buf, w, h, image::ColorType::Rgba8).expect("png written");
        assert!(out.exists() && out.metadata().unwrap().len() > 1000);
    }
}