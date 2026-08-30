//! `.desktop` entry scanning for `--run` (program launcher) mode.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct App {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
}

const CHECK_KEYS: &[&str] = &["Name", "Icon", "Exec", "NoDisplay", "Hidden"];

/// All application directories per the freedesktop spec, lowest to highest priority.
pub fn app_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(h) = std::env::var("XDG_DATA_HOME") {
        v.push(PathBuf::from(h).join("applications"));
    } else if let Ok(h) = std::env::var("HOME") {
        v.push(PathBuf::from(&h).join(".local/share/applications"));
    }
    if let Ok(d) = std::env::var("XDG_DATA_DIRS") {
        for p in d.split(':') {
            if !p.is_empty() {
                v.push(PathBuf::from(p).join("applications"));
            }
        }
    }
    v.push(PathBuf::from("/usr/local/share/applications"));
    v.push(PathBuf::from("/usr/share/applications"));
    v
}

/// Scan desktop dirs, first-priority wins per file name (freedesktop dedup rule).
pub fn load_apps() -> Vec<App> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut apps = Vec::new();
    for dir in app_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let Some(file) = path.file_name().and_then(|f| f.to_str()).map(str::to_string) else {
                continue;
            };
            if !seen.insert(file) {
                continue;
            }
            if let Some(app) = parse_desktop(&path) {
                apps.push(app);
            }
        }
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

fn parse_desktop(path: &Path) -> Option<App> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let mut name = None;
    let mut icon = None;
    let mut exec = None;
    let mut no_display = false;
    let mut hidden = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        if !CHECK_KEYS.contains(&key) {
            continue;
        }
        match key {
            "Name" => name = Some(value.to_string()),
            "Icon" => icon = Some(value.to_string()),
            "Exec" => exec = Some(value.to_string()),
            "NoDisplay" => no_display = value == "true",
            "Hidden" => hidden = value == "true",
            _ => {}
        }
    }
    if no_display || hidden {
        return None;
    }
    Some(App {
        name: name?,
        exec: clean_exec(exec.as_deref()?),
        icon: icon.filter(|i| !i.is_empty()),
    })
}

/// Strip trailing field codes (e.g. `%F %u`) from an Exec line.
fn clean_exec(exec: &str) -> String {
    let words: Vec<&str> = exec.split_whitespace().collect();
    let end = words.iter().rposition(|w| !w.starts_with('%')).map_or(0, |i| i + 1);
    words[..end].join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_exec_strips_field_codes() {
        assert_eq!(clean_exec("firefox %u"), "firefox");
        assert_eq!(clean_exec("alacritty -e fish %F"), "alacritty -e fish");
        assert_eq!(clean_exec("code --new-window %F %U"), "code --new-window");
        assert_eq!(clean_exec("env FOO=1 app --flag"), "env FOO=1 app --flag");
    }

    #[test]
    fn parses_desktop_file() {
        let dir = std::env::temp_dir().join("rmenu-test-apps");
        let _ = std::fs::create_dir_all(&dir);
        let df = dir.join("test-app.desktop");
        std::fs::write(
            &df,
            "[Desktop Entry]\nType=Application\nName=Test 应用\nIcon=org.test.App\n\
             Exec=test-app --flag %U\nNoDisplay=false\n",
        )
        .unwrap();
        let app = parse_desktop(&df).expect("parses");
        assert_eq!(app.name, "Test 应用");
        assert_eq!(app.exec, "test-app --flag");
        assert_eq!(app.icon.as_deref(), Some("org.test.App"));

        std::fs::write(&df, "[Desktop Entry]\nType=Application\nName=Hide Me\nNoDisplay=true\n").unwrap();
        assert!(parse_desktop(&df).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scans_system_dirs() {
        let apps = load_apps();
        assert!(!apps.is_empty(), "system /usr/share/applications should yield entries");
        assert!(apps.windows(2).all(|w| w[0].name.to_lowercase() <= w[1].name.to_lowercase()));
    }
}