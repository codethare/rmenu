//! `.desktop` entry scanning for `--run` (program launcher) mode.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct App {
    pub name: String,
    pub exec: String,
}

const CHECK_KEYS: &[&str] = &["Name", "Exec", "NoDisplay", "Hidden"];

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
            "Exec" => exec = Some(value.to_string()),
            "NoDisplay" => no_display = value == "true",
            "Hidden" => hidden = value == "true",
            _ => {}
        }
    }
    if no_display || hidden {
        return None;
    }
    Some(App { name: name?, exec: clean_exec(exec.as_deref()?) })
}

/// Strip trailing field codes (e.g. `%F %u`) from an Exec line.
fn clean_exec(exec: &str) -> String {
    let words: Vec<&str> = exec.split_whitespace().collect();
    let end = words.iter().rposition(|w| !w.starts_with('%')).map_or(0, |i| i + 1);
    words[..end].join(" ")
}

/// Executables found in `$PATH` (scripts, binaries, symlinks), deduped by file name.
pub fn path_commands() -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in path.split(':').filter(|d| !d.is_empty()) {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !seen.insert(name.clone()) {
                continue;
            }
            if is_executable(&e.path()) {
                out.push(name);
            }
        }
    }
    out
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Merge desktop apps and PATH commands into menu items: apps win on name
/// collision (case-insensitive), result sorted by name.
pub fn merged(apps: Vec<App>, cmds: Vec<String>) -> Vec<crate::items::Item> {
    let mut items: Vec<crate::items::Item> = apps
        .into_iter()
        .map(|a| crate::items::Item { lc: a.name.to_lowercase(), text: a.name, value: a.exec })
        .collect();
    let mut seen: HashSet<String> = items.iter().map(|i| i.text.to_lowercase()).collect();
    for cmd in cmds {
        if seen.insert(cmd.to_lowercase()) {
            items.push(crate::items::Item { lc: cmd.to_lowercase(), text: cmd.clone(), value: cmd });
        }
    }
    items.sort_by(|a, b| a.text.to_lowercase().cmp(&b.text.to_lowercase()));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_apps_and_commands_dedups_by_name() {
        let apps = vec![
            App { name: "Firefox".into(), exec: "firefox %u".into() },
            App { name: "Zathura".into(), exec: "zathura %f".into() },
        ];
        let cmds = vec!["firefox".to_string(), "alacritty".to_string(), "zathura".to_string()];
        let items = merged(apps, cmds);
        let names: Vec<&str> = items.iter().map(|i| i.text.as_str()).collect();
        // deduped, sorted, desktop entry wins over the bare command
        assert_eq!(names, vec!["alacritty", "Firefox", "Zathura"]);
    }

    #[test]
    fn path_commands_finds_shell_utilities() {
        let cmds = path_commands();
        assert!(cmds.iter().any(|c| c == "sh"), "PATH should contain sh: {cmds:?}");
        assert!(cmds.iter().any(|c| c == "ls" || c == "env" || c == "cat"));
    }
}

#[cfg(test)]
mod legacy_tests {
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