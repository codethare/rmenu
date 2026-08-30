//! Font loading: an explicit font path, or an auto-picked system font
//! (CJK-capable preferred so Chinese labels render).

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};

pub struct MenuFont {
    pub font: FontVec,
    /// Pixel size (em height).
    pub size: f32,
    pub ascent_px: f32,
    pub descent_px: f32,
    pub line_h: f32,
    /// Row height incl. padding.
    pub row_h: u32,
}

impl MenuFont {
    pub fn load(path: Option<&str>, size: f32) -> Result<MenuFont, String> {
        let (data, index) = match path {
            Some(p) => {
                let bytes = std::fs::read(p).map_err(|e| format!("cannot read font {p}: {e}"))?;
                (bytes, 0)
            }
            None => pick_system_font().ok_or("no usable system font found".to_string())?,
        };
        let font = FontVec::try_from_vec_and_index(data, index)
            .map_err(|e| format!("invalid font: {e}"))?;
        let scaled = font.as_scaled(PxScale::from(size));
        let ascent_px = scaled.ascent();
        let descent_px = scaled.descent();
        let line_h = scaled.height(); // == size by definition of PxScale
        // Roomier rows so icons (row_h - 8 px) are clearly visible.
        let row_h = (line_h * 2.0).ceil() as u32;
        Ok(MenuFont { font, size, ascent_px, descent_px, line_h, row_h })
    }
}

fn pick_system_font() -> Option<(Vec<u8>, u32)> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let families: &[&str] = &[
        "Noto Sans CJK SC",
        "Noto Sans CJK TC",
        "Noto Sans CJK JP",
        "WenQuanYi Micro Hei",
        "Source Han Sans SC",
        "Noto Sans Mono",
        "DejaVu Sans Mono",
        "Liberation Mono",
    ];
    for fam in families {
        if let Some(id) =
            db.query(&fontdb::Query { families: &[fontdb::Family::Name(fam)], ..Default::default() })
        {
            if let Some(found) = copy_face(&db, id) {
                return Some(found);
            }
        }
    }
    // Give up on family preference: just take the first face so the menu still renders.
    let id = db.faces().next()?.id;
    copy_face(&db, id)
}

fn copy_face(db: &fontdb::Database, id: fontdb::ID) -> Option<(Vec<u8>, u32)> {
    db.with_face_data(id, |data, index| (data.to_vec(), index))
}