//! dmenu/wmenu-style items: `<icon>\t<text>` input lines and token substring filtering.

#[derive(Debug, Clone)]
pub struct Item {
    /// Icon path or theme icon name; `None` when the line had no tab field.
    pub icon: Option<String>,
    /// Display text (everything after the first tab, or the whole line).
    pub text: String,
    /// Value printed to stdout when selected (the text, sans icon field).
    pub value: String,
}

pub fn parse(line: &str) -> Item {
    match line.split_once('\t') {
        Some((icon, rest)) => Item {
            icon: Some(icon.trim().to_string()),
            text: rest.to_string(),
            value: rest.to_string(),
        },
        None => Item {
            icon: None,
            text: line.to_string(),
            value: line.to_string(),
        },
    }
}

fn contains(haystack: &str, needle: &str, ci: bool) -> bool {
    if ci {
        haystack.to_lowercase().contains(&needle.to_lowercase())
    } else {
        haystack.contains(needle)
    }
}

fn starts_with(haystack: &str, needle: &str, ci: bool) -> bool {
    if ci {
        haystack.to_lowercase().starts_with(&needle.to_lowercase())
    } else {
        haystack.starts_with(needle)
    }
}

/// wmenu-style matching: every whitespace-separated query token must be a substring
/// of the item text; ranking is exact > prefix > substring, stable within each bucket.
/// Returns indices into `items`. An empty query matches everything, in order.
pub fn filter(items: &[Item], query: &str, ci: bool) -> Vec<usize> {
    let toks: Vec<&str> = query.split_whitespace().collect();
    let mut exact = Vec::new();
    let mut prefix = Vec::new();
    let mut sub = Vec::new();
    for (i, it) in items.iter().enumerate() {
        if !toks.iter().all(|t| contains(&it.text, t, ci)) {
            continue;
        }
        let bucket = match toks.len() {
            0 => &mut exact,
            _ if starts_with(&it.text, query, ci) => &mut exact,
            _ if starts_with(&it.text, toks[0], ci) => &mut prefix,
            _ => &mut sub,
        };
        bucket.push(i);
    }
    exact.into_iter().chain(prefix).chain(sub).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(src: &[&str]) -> Vec<Item> {
        src.iter().map(|s| parse(s)).collect()
    }

    #[test]
    fn parse_splits_tab_icon() {
        let it = parse("firefox\tFirefox 火狐");
        assert_eq!(it.icon.as_deref(), Some("firefox"));
        assert_eq!(it.text, "Firefox 火狐");
        assert_eq!(it.value, "Firefox 火狐");
        let plain = parse("just a line");
        assert!(plain.icon.is_none());
        assert_eq!(plain.value, "just a line");
    }

    #[test]
    fn filter_rank_exact_prefix_substr() {
        let it = items(&["libre/server", "libre", "LIBREoffice", "libreoffice", "not-libre"]);
        let m = filter(&it, "libre", false);
        let got: Vec<&str> = m.iter().map(|&i| it[i].text.as_str()).collect();
        // exact prefix first (in original order), then substring matches; "LIBREoffice"
        // is case-mismatched so it is not a substring match here.
        assert_eq!(got, vec!["libre/server", "libre", "libreoffice", "not-libre"]);
    }

    #[test]
    fn filter_empty_query_returns_all() {
        let it = items(&["a", "b", "c"]);
        assert_eq!(filter(&it, "", false), vec![0, 1, 2]);
    }

    #[test]
    fn filter_multiple_tokens_and() {
        let it = items(&["Firefox web browser", "Firefox", "Terminal emulator"]);
        assert_eq!(filter(&it, "fire web", true), vec![0]);
    }

    #[test]
    fn filter_case_flags() {
        let it = items(&["Firefox"]);
        assert!(filter(&it, "fire", false).is_empty());
        assert_eq!(filter(&it, "fire", true), vec![0]);
    }
}