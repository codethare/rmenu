//! Icon loading: direct file paths (png/jpg/webp/gif via `image`, svg via `resvg`)
//! or freedesktop icon-theme name lookup.

use image::{imageops, RgbaImage};

/// Sizes to try, best quality first; `scalable` (svg) last as a fallback.
const SIZES: &[&str] = &["48x48", "64x64", "96x96", "32x32", "128x128", "24x24", "22x22", "16x16", "256x256"];
const CATS: &[&str] = &["apps", "mimetypes", "places", "status", "devices", "actions", "legacy"];
const EXTS: &[&str] = &["png", "svg", "jpg", "jpeg", "webp"];

/// Resolve `spec` (a path or an icon theme name) and return it scaled to `target` px (square).
pub fn load(spec: &str, target: u32) -> Option<RgbaImage> {
    if spec.contains('/') {
        return load_file(spec, target);
    }
    let path = find_in_theme(spec)?;
    load_file(&path, target)
}

/// Decode an image file, scaled so the longest side is `target` px.
fn load_file(path: &str, target: u32) -> Option<RgbaImage> {
    let data = std::fs::read(path).ok()?;
    // Raster formats first; svg only when the bytes aren't a raster image.
    if let Ok(img) = image::load_from_memory(&data) {
        return Some(scale(&img.to_rgba8(), target));
    }
    if path.ends_with(".svg") {
        return load_svg(&data, target);
    }
    None
}

fn load_svg(data: &[u8], target: u32) -> Option<RgbaImage> {
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let w = size.width();
    let h = size.height();
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let scale = target as f32 / w.max(h);
    let pw = (w * scale).round().max(1.0) as u32;
    let ph = (h * scale).round().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph)?;
    let mut pm = pixmap.as_mut();
    resvg::render(&tree, usvg::Transform::from_scale(scale, scale), &mut pm);
    // tiny_skia data is premultiplied RGBA; un-premultiply for straight-alpha blending.
    let mut out = RgbaImage::new(pw, ph);
    for (dst, src) in out.pixels_mut().zip(pixmap.data().chunks_exact(4)) {
        let a = src[3] as u32;
        dst.0 = [
            if a == 0 { 0 } else { src[0] as u32 * 255 / a } as u8,
            if a == 0 { 0 } else { src[1] as u32 * 255 / a } as u8,
            if a == 0 { 0 } else { src[2] as u32 * 255 / a } as u8,
            src[3],
        ];
    }
    Some(out)
}

fn scale(img: &RgbaImage, target: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    if w <= target && h <= target {
        return img.clone();
    }
    let f = target as f32 / w.max(h) as f32;
    imageops::resize(
        img,
        ((w as f32 * f).round().max(1.0)) as u32,
        ((h as f32 * f).round().max(1.0)) as u32,
        imageops::FilterType::Triangle,
    )
}

fn icon_dirs() -> Vec<std::path::PathBuf> {
    let mut v = Vec::new();
    if let Ok(h) = std::env::var("XDG_DATA_HOME") {
        v.push(std::path::PathBuf::from(h).join("icons"));
    } else if let Ok(h) = std::env::var("HOME") {
        v.push(std::path::PathBuf::from(&h).join(".local/share/icons"));
    }
    if let Ok(d) = std::env::var("XDG_DATA_DIRS") {
        for p in d.split(':') {
            if !p.is_empty() {
                v.push(std::path::PathBuf::from(p).join("icons"));
            }
        }
    }
    v.push(std::path::PathBuf::from("/usr/local/share/icons"));
    v.push(std::path::PathBuf::from("/usr/share/icons"));
    v.push(std::path::PathBuf::from("/usr/share/pixmaps"));
    v
}

fn has_known_ext(name: &str) -> bool {
    EXTS.iter().any(|e| name.ends_with(&format!(".{e}")))
}

/// Search the standard icon theme directories for `name`.
fn find_in_theme(name: &str) -> Option<String> {
    let current = std::env::var("XDG_CURRENT_DESKTOP").ok();
    for dir in icon_dirs() {
        let mut themes: Vec<String> = Vec::new();
        for t in [current.as_deref(), Some("Adwaita"), Some("hicolor")].into_iter().flatten() {
            if !themes.iter().any(|x| x == t) {
                themes.push(t.to_string());
            }
        }
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut rest: Vec<String> = rd
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|t| !themes.iter().any(|x| x == t))
                .collect();
            rest.sort();
            themes.extend(rest);
        }
        for theme in &themes {
            if has_known_ext(name) {
                // Icon is a file name: look inside any size/category and pixmaps.
                for size in SIZES {
                    for cat in CATS {
                        let p = dir.join(theme).join(size).join(cat).join(name);
                        if p.is_file() {
                            return p.to_str().map(str::to_string);
                        }
                    }
                }
                let p = dir.join(name);
                if p.is_file() {
                    return p.to_str().map(str::to_string);
                }
            } else {
                for size in SIZES {
                    for cat in CATS {
                        if let Some(p) = check(&dir, theme, size, cat, name, EXTS) {
                            return Some(p);
                        }
                    }
                }
                if let Some(p) = check(&dir, theme, "scalable", "apps", name, &["svg"]) {
                    return Some(p);
                }
            }
        }
        // Flat fallback per dir: `<dir>/<name>.<ext>` (covers /usr/share/pixmaps,
        // e.g. Icon=Alacritty with only Alacritty.svg in pixmaps).
        for ext in EXTS {
            let p = dir.join(format!("{name}.{ext}"));
            if p.is_file() {
                return p.to_str().map(str::to_string);
            }
        }
    }
    None
}

fn theme_dirs<'a>(size: &str, theme: &'a str) -> [std::path::PathBuf; 1] {
    [std::path::PathBuf::from(theme).join(size)]
}

fn check(dir: &std::path::Path, theme: &str, size: &str, cat: &str, name: &str, exts: &[&str]) -> Option<String> {
    for base in theme_dirs(size, theme) {
        let base = dir.join(base);
        for ext in exts {
            let p = base.join(cat).join(format!("{name}.{ext}"));
            if p.is_file() {
                return p.to_str().map(str::to_string);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_theme_name_and_path() {
        // hicolor ships the fcitx icons on this system; skip if not present.
        let named = load("fcitx-wbpy", 32);
        if std::path::Path::new("/usr/share/icons/hicolor/48x48/apps/fcitx-wbpy.png").exists() {
            let named = named.expect("theme-name lookup should find fcitx-wbpy");
            assert_eq!(named.width(), 32);
            assert_eq!(named.height(), 32);
        }
    }

    #[test]
    fn resolves_direct_path() {
        let p = "/usr/share/icons/hicolor/48x48/apps/fcitx-wbpy.png";
        if std::path::Path::new(p).exists() {
            let img = load(p, 24).expect("direct path should load");
            assert_eq!(img.width(), 24);
        }
    }

    #[test]
    fn missing_icon_is_none() {
        assert!(load("definitely-not-an-icon-name-xyz", 32).is_none());
    }

    #[test]
    fn resolves_pixmaps_svg_by_theme_name() {
        // e.g. Alacritty ships only /usr/share/pixmaps/Alacritty.svg on Arch.
        let pixmap = std::path::Path::new("/usr/share/pixmaps/Alacritty.svg");
        if pixmap.exists() {
            let img = load("Alacritty", 24).expect("pixmaps svg resolves by theme name");
            assert_eq!(img.width(), 24);
            assert_eq!(img.height(), 24);
        }
    }
}
