//! Font loading: an explicit font path, or an auto-picked system font
//! (CJK-capable preferred so Chinese labels render).

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use std::path::{Path, PathBuf};
#[cfg(test)]
use ab_glyph::GlyphId;

pub struct MenuFont {
    pub font: FontVec,
    /// Extra faces tried in order when the primary lacks a glyph (CJK etc.).
    pub fallbacks: Vec<FontVec>,
    /// Pixel size (em height).
    pub size: f32,
    pub ascent_px: f32,
    pub descent_px: f32,
    pub line_h: f32,
    /// Row height incl. padding.
    pub row_h: u32,
}

impl MenuFont {
    pub fn load(spec: Option<&str>, size: f32) -> Result<MenuFont, String> {
        let spec = spec.map(str::trim);
        // `-f /path/to/font.ttf`: use exactly that face — no system font scan,
        // no fallback chain (wmenu -f semantics). This is the fast-startup
        // path: building the system chain scans every installed font (~100ms).
        if let Some(s) = spec {
            if let Ok(bytes) = std::fs::read(s) {
                let font = FontVec::try_from_vec(bytes).map_err(|e| format!("invalid font: {e}"))?;
                return Ok(Self::build(font, Vec::new(), size));
            }
        }

        let chain = cached_system_chain();
        let (bytes, index, size) = match spec {
            Some(s) => resolve_family(s, size)?,
            None => {
                let (path, index) =
                    chain.first().cloned().ok_or("no usable system font found".to_string())?;
                let bytes =
                    std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
                (bytes, index, size)
            }
        };
        let font = FontVec::try_from_vec_and_index(bytes, index)
            .map_err(|e| format!("invalid font: {e}"))?;
        // With no `-f` the chain's head is the primary; skip it in the fallbacks.
        let skip = usize::from(spec.is_none());
        let fallbacks: Vec<FontVec> = chain
            .into_iter()
            .skip(skip)
            .filter_map(|(path, i)| {
                let bytes = std::fs::read(path).ok()?;
                FontVec::try_from_vec_and_index(bytes, i).ok()
            })
            .collect();
        Ok(Self::build(font, fallbacks, size))
    }

    fn build(font: FontVec, fallbacks: Vec<FontVec>, size: f32) -> MenuFont {
        let scaled = font.as_scaled(PxScale::from(size));
        let ascent_px = scaled.ascent();
        let descent_px = scaled.descent();
        let line_h = scaled.height(); // == size by definition of PxScale
        // Comfortable row height ≈1.5× font size: glyphs breathe top/bottom,
        // and the tall selection band is an easier target to hit (Fitts).
        const ROW_VPAD: u32 = 4;
        let row_h = line_h.ceil() as u32 + 2 * ROW_VPAD;
        MenuFont { font, fallbacks, size, ascent_px, descent_px, line_h, row_h }
    }

    /// True if any face in the chain (primary first) has a glyph for `ch`.
    #[cfg(test)]
    pub fn has_glyph(&self, ch: char) -> bool {
        std::iter::once(&self.font)
            .chain(self.fallbacks.iter())
            .any(|f| f.as_scaled(PxScale::from(self.size)).glyph_id(ch) != GlyphId(0))
    }
}

/// `-f` family-style spec: "SourceCodePro medium 13" — family, optional style
/// word, optional size (points, or pixels with a "px" suffix; Pango/wmenu
/// convention). Resolved through the full system font database.
fn resolve_family(spec: &str, default_size: f32) -> Result<(Vec<u8>, u32, f32), String> {
    let (family, weight, size) = parse_spec(spec);
    query_family(&family, weight)
        .map(|(data, index)| (data, index, size.unwrap_or(default_size)))
        .ok_or_else(|| format!("font family not found: {family}"))
}

/// Split "Family [style] [size]": the first numeric token is the size
/// (points, or pixels with a "px" suffix); recognized style words pick the
/// weight; everything else is the family. Returned size is device pixels.
fn parse_spec(spec: &str) -> (String, fontdb::Weight, Option<f32>) {
    let mut family = Vec::new();
    let mut weight = fontdb::Weight::NORMAL;
    let mut size = None;
    for tok in spec.split_whitespace() {
        if size.is_none() {
            if let Some(s) = size_in_px(tok) {
                size = Some(s);
                continue;
            }
        }
        if let Some(w) = weight_of(tok) {
            weight = w;
        } else {
            family.push(tok);
        }
    }
    (family.join(" "), weight, size)
}

/// Pango-aligned size token → pixel size: a bare number is points, converted
/// at Pango's default 96 dpi (×96/72); a "px" suffix is absolute pixels.
fn size_in_px(tok: &str) -> Option<f32> {
    if let Some(px) = tok.to_ascii_lowercase().strip_suffix("px") {
        return px.trim().parse::<f32>().ok();
    }
    tok.parse::<f32>().ok().map(|pt| pt * 96.0 / 72.0)
}

fn weight_of(word: &str) -> Option<fontdb::Weight> {
    match word.to_ascii_lowercase().as_str() {
        "thin" => Some(fontdb::Weight::THIN),
        "light" => Some(fontdb::Weight::LIGHT),
        "normal" | "regular" | "book" | "roman" => Some(fontdb::Weight::NORMAL),
        "medium" => Some(fontdb::Weight::MEDIUM),
        "semibold" | "demibold" => Some(fontdb::Weight::SEMIBOLD),
        "bold" => Some(fontdb::Weight::BOLD),
        "extrabold" | "ultrabold" | "heavy" => Some(fontdb::Weight::EXTRA_BOLD),
        "black" => Some(fontdb::Weight::BLACK),
        _ => None,
    }
}

fn query_family(family: &str, weight: fontdb::Weight) -> Option<(Vec<u8>, u32)> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let fam = fontdb::Family::Name(family);
    let q =
        |w| fontdb::Query { families: std::slice::from_ref(&fam), weight: w, ..Default::default() };
    db.query(&q(weight))
        .or_else(|| db.query(&q(fontdb::Weight::NORMAL)))
        .and_then(|id| copy_face(&db, id))
}

/// CJK-preferring ordered list of usable system font paths (primary candidate
/// first), used both for auto-pick and as the fallback chain.
fn system_chain() -> Vec<(PathBuf, u32)> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    // CJK list: mutual alternates, not complements. One face covers all CJK +
    // kana glyphs, and SC/TC/JP are usually faces of the same ~20MB TTC — so
    // take the first hit and stop.
    const CJK: &[&str] = &[
        "Noto Sans CJK SC",
        "Noto Sans CJK TC",
        "Noto Sans CJK JP",
        "WenQuanYi Micro Hei",
        "Source Han Sans SC",
    ];
    const MONO: &[&str] = &["Noto Sans Mono", "DejaVu Sans Mono", "Liberation Mono"];
    let mut out = Vec::new();
    for fam in CJK {
        if let Some(found) = query_path(&db, fam) {
            out.push(found);
            break;
        }
    }
    for fam in MONO {
        if let Some(found) = query_path(&db, fam) {
            out.push(found);
        }
    }
    // Nerd Font PUA icons (patched families / "Symbols Nerd Font"): the mono
    // and CJK faces never carry -style glyphs, so a face whose family mentions
    // "Nerd Font" joins the chain to resolve them.
    for face in db.faces() {
        if face.families.iter().any(|f| f.0.to_ascii_lowercase().contains("nerd font"))
            && let Some(found) = face_path(&db, face.id)
        {
            out.push(found);
            break;
        }
    }
    if out.is_empty() {
        // Give up on family preference: just take the first face so the menu still renders.
        if let Some(face) = db.faces().next()
            && let Some(found) = face_path(&db, face.id)
        {
            out.push(found);
        }
    }
    out
}

fn query_path(db: &fontdb::Database, family: &str) -> Option<(PathBuf, u32)> {
    let id = db.query(&fontdb::Query { families: &[fontdb::Family::Name(family)], ..Default::default() })?;
    face_path(db, id)
}

fn face_path(db: &fontdb::Database, id: fontdb::ID) -> Option<(PathBuf, u32)> {
    let face = db.face(id)?;
    let path = match &face.source {
        fontdb::Source::File(p) | fontdb::Source::SharedFile(p, _) => p.clone(),
        fontdb::Source::Binary(_) => return None,
    };
    Some((path, face.index))
}

/// Persistent cache of the resolved system chain, so the ~100ms fontdb scan
/// (it parses every installed font header on each launch; on this box that is
/// 2193 files / 756MB) only runs when fonts actually change. Keyed on the
/// standard font dirs' mtimes — the same trick as fontconfig's fc-cache.
/// ponytail: dirs from a custom /etc/fonts/fonts.conf aren't keyed, so chain
/// changes there go unnoticed until a keyed dir changes; `-f FAMILY` rescans anyway.
fn cached_system_chain() -> Vec<(PathBuf, u32)> {
    let (Some(key), Some(path)) = (cache_key(), cache_path()) else {
        return system_chain(); // no XDG_CACHE_HOME/HOME — scan every launch
    };
    if let Some(chain) = read_cache(&path, &key)
        && chain.iter().all(|(p, _)| p.exists())
    {
        return chain;
    }
    let chain = system_chain();
    write_cache(&path, &key, &chain);
    chain
}

/// Standard user/system font dirs (fontdb's no-fontconfig scan list); their
/// mtimes are the cache key. A font added/removed touches the dir mtime.
fn cache_key() -> Option<Vec<(PathBuf, std::time::SystemTime)>> {
    let mut dirs = Vec::new();
    if let Some(h) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(h).join("fonts"));
    } else if let Some(h) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(h).join(".fonts"));
    }
    if let Some(h) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(h).join(".local/share/fonts"));
    }
    dirs.push(PathBuf::from("/usr/local/share/fonts"));
    dirs.push(PathBuf::from("/usr/share/fonts"));
    let key: Vec<_> = dirs
        .into_iter()
        .filter_map(|d| Some((d.clone(), std::fs::metadata(&d).ok()?.modified().ok()?)))
        .collect();
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

fn cache_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("rmenu").join("font-chain"))
}

fn mtime_parts(t: std::time::SystemTime) -> (u64, u32) {
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => (d.as_secs(), d.subsec_nanos()),
        Err(_) => (0, 0), // pre-epoch clock: constant key
    }
}

/// `None` = cache missing, unparsable, or keyed to different fonts.
fn read_cache(path: &Path, key: &[(PathBuf, std::time::SystemTime)]) -> Option<Vec<(PathBuf, u32)>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()? != "rmenu-font-cache v1" {
        return None;
    }
    for (dir, mt) in key {
        let mut it = lines.next()?.splitn(4, '\t');
        let (tag, d, s, n) = (
            it.next()?,
            it.next()?,
            it.next()?.parse::<u64>().ok()?,
            it.next()?.parse::<u32>().ok()?,
        );
        if tag != "d" || d != dir.to_str()? || (s, n) != mtime_parts(*mt) {
            return None;
        }
    }
    let mut chain = Vec::new();
    for line in lines {
        let mut it = line.splitn(3, '\t');
        if it.next()? != "f" {
            return None;
        }
        let index: u32 = it.next()?.parse().ok()?;
        chain.push((PathBuf::from(it.next()?), index));
    }
    if chain.is_empty() {
        return None;
    }
    Some(chain)
}

/// Best-effort: any I/O failure just means the next launch rescans.
fn write_cache(path: &Path, key: &[(PathBuf, std::time::SystemTime)], chain: &[(PathBuf, u32)]) {
    if chain.is_empty() {
        return;
    }
    let mut text = String::from("rmenu-font-cache v1\n");
    for (dir, mt) in key {
        let Some(d) = dir.to_str() else { return };
        let (s, n) = mtime_parts(*mt);
        text.push_str(&format!("d\t{d}\t{s}\t{n}\n"));
    }
    for (p, i) in chain {
        let Some(p) = p.to_str() else { return };
        if p.contains('\n') {
            return;
        }
        text.push_str(&format!("f\t{i}\t{p}\n"));
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, path); // atomic: never serve a torn cache
    }
}

fn copy_face(db: &fontdb::Database, id: fontdb::ID) -> Option<(Vec<u8>, u32)> {
    db.with_face_data(id, |data, index| (data.to_vec(), index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_family_spec_extracts_name_weight_size() {
        // 13 pt (Pango/wmenu convention) at 96 dpi.
        let (fam, w, sz) = parse_spec("SourceCodePro medium 13");
        assert_eq!(fam, "SourceCodePro");
        assert_eq!(w, fontdb::Weight::MEDIUM);
        assert_eq!(sz, Some(13.0 * 96.0 / 72.0));
    }

    #[test]
    fn px_suffix_is_absolute_pixels_not_points() {
        let (_, _, sz) = parse_spec("monospace 12px");
        assert_eq!(sz, Some(12.0));
        // The same bare number is points: 12 pt == 16 px at 96 dpi.
        let (_, _, sz) = parse_spec("monospace 12");
        assert_eq!(sz, Some(16.0));
    }

    #[test]
    fn load_wmenu_style_family_with_weight_and_size() {
        // The documented `-f "Noto Sans Mono Medium 12"` form: 12 pt == 16 px
        // at 96 dpi, so it renders identically to wmenu.
        let m = MenuFont::load(Some("Noto Sans Mono Medium 12"), 16.0).expect("medium face found");
        assert_eq!(m.size, 16.0);
        assert!(m.has_glyph('a'));
    }

    #[test]
    fn file_spec_loads_exactly_that_font_without_chain() {
        // `-f /path.ttf` must not build the system chain: no fallbacks at all
        // (wmenu -f semantics) — this is the fast-startup path.
        let is_font = |p: &std::path::Path| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_ascii_lowercase())
                    .is_some_and(|e| e == "ttf" || e == "otf" || e == "ttc")
        };
        let root = std::path::Path::new("/usr/share/fonts");
        let mut file = None;
        for entry in std::fs::read_dir(root).unwrap().flatten() {
            let p = entry.path();
            if is_font(&p) {
                file = Some(p);
                break;
            }
            if p.is_dir() {
                file = std::fs::read_dir(&p).unwrap().flatten().map(|e| e.path()).find(|q| is_font(q));
                if file.is_some() {
                    break;
                }
            }
        }
        let file = file.expect("a ttf/otf/ttc under /usr/share/fonts");
        let m = MenuFont::load(file.to_str(), 16.0).expect("font file loads");
        assert!(m.fallbacks.is_empty(), "explicit file means no fallback chain");
        assert_eq!(m.size, 16.0);
    }

    #[test]
    fn chain_takes_one_cjk_face_only() {
        // SC/TC/JP/文泉驿/思源 are alternates, not complements (usually faces of
        // the same ~20MB TTC): the chain must never hold more than one big face.
        // Threshold is above any nerd-font/mono face (<8MB) but below a CJK TTC.
        let chain = system_chain();
        let big = chain
            .iter()
            .filter(|(p, _)| std::fs::metadata(p).map(|m| m.len() > 8_000_000).unwrap_or(false))
            .count();
        assert!(big <= 1, "at most one large (CJK) face in chain: {chain:?}");
    }

    #[test]
    fn font_cache_round_trips_and_invalidates_on_key_change() {
        let dir = std::env::temp_dir().join(format!("rmenu-cache-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let font = dir.join("a.ttf");
        std::fs::write(&font, b"font").unwrap();
        let path = dir.join("font-chain");
        let key = vec![(font.clone(), std::fs::metadata(&font).unwrap().modified().unwrap())];
        let chain = vec![(font.clone(), 3u32)];
        write_cache(&path, &key, &chain);
        assert_eq!(read_cache(&path, &key), Some(chain));
        // A changed dir mtime (font added/removed) invalidates the cache.
        let stale = vec![(
            font.clone(),
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000),
        )];
        assert_eq!(read_cache(&path, &stale), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chain_renders_nerd_font_pua_icons() {
        // Skip on systems without any Nerd Font (no PUA icons to draw anyway).
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let sample = db.faces().find_map(|f| {
            if !f.families.iter().any(|f| f.0.to_ascii_lowercase().contains("nerd font")) {
                return None;
            }
            db.with_face_data(f.id, |d, i| (d.to_vec(), i))
                .and_then(|(d, i)| FontVec::try_from_vec_and_index(d, i).ok())
        });
        let Some(fv) = sample else { return };
        // Pick one PUA codepoint the installed Nerd face actually covers.
        let scaled = fv.as_scaled(PxScale::from(16.0));
        let Some(cp) = (0xF000..=0xFDFF)
            .find(|&cp| scaled.glyph_id(char::from_u32(cp).unwrap()) != GlyphId(0))
        else { return };
        let m = MenuFont::load(None, 16.0).expect("system font available");
        assert!(
            m.has_glyph(char::from_u32(cp).unwrap()),
            "Nerd Font PUA glyph U+{cp:X} must resolve via the chain"
        );
    }

    #[test]
    fn parse_family_plain_and_multiword() {
        let (fam, w, sz) = parse_spec("monospace");
        assert_eq!(fam, "monospace");
        assert_eq!(w, fontdb::Weight::NORMAL);
        assert_eq!(sz, None);

        let (fam, w, sz) = parse_spec("Source Code Pro bold 12.5");
        assert_eq!(fam, "Source Code Pro");
        assert_eq!(w, fontdb::Weight::BOLD);
        assert_eq!(sz, Some(12.5 * 96.0 / 72.0));
    }

    #[test]
    fn unknown_style_word_stays_in_family() {
        let (fam, _, _) = parse_spec("Fira Code Extra");
        assert_eq!(fam, "Fira Code Extra");
    }

    #[test]
    fn fallback_chain_covers_cjk_for_latin_primary() {
        // Noto Sans Mono has no CJK glyphs; the chain must resolve them.
        let m = MenuFont::load(Some("Noto Sans Mono"), 13.0).expect("system font available");
        let latin = m.font.as_scaled(PxScale::from(m.size)).glyph_id('a');
        assert_ne!(latin, GlyphId(0));
        assert!(m.has_glyph('a'));
        assert!(
            m.has_glyph('中'),
            "CJK must resolve via the fallback chain when the primary is latin-only"
        );
    }
}