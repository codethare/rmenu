//! dmenu/wmenu-style items: stdin text lines and token substring filtering.

#[derive(Debug, Clone)]
pub struct Item {
    /// Display text.
    pub text: String,
    /// Value printed to stdout when selected.
    pub value: String,
    /// Lowercased copy of `text`, precomputed so `-i` filtering never
    /// lowercases per item per token on every keystroke.
    pub lc: String,
}

pub fn parse(line: &str) -> Item {
    Item { lc: line.to_lowercase(), text: line.to_string(), value: line.to_string() }
}

fn contains(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}

fn starts_with(haystack: &str, needle: &str) -> bool {
    haystack.starts_with(needle)
}

/// wmenu-style matching: every whitespace-separated query token must be a substring
/// of the item text; ranking is exact > prefix > substring, stable within each bucket.
/// Returns indices into `items`. An empty query matches everything, in order.
pub fn filter(items: &[Item], query: &str, ci: bool) -> Vec<usize> {
    // Hot path (per keystroke): lowercase the query once instead of per item
    // per token; each item's lowercase copy was precomputed at parse time.
    if query.is_empty() {
        return (0..items.len()).collect();
    }
    let query = if ci { query.to_lowercase() } else { query.to_string() };
    let toks: Vec<&str> = query.split_whitespace().collect();
    let mut exact = Vec::new();
    let mut prefix = Vec::new();
    let mut sub = Vec::new();
    for (i, it) in items.iter().enumerate() {
        let hay = if ci { &it.lc } else { &it.text };
        if !toks.iter().all(|t| contains(hay, t)) {
            continue;
        }
        let bucket = match toks.len() {
            0 => &mut exact,
            _ if starts_with(hay, &query) => &mut exact,
            _ if starts_with(hay, toks[0]) => &mut prefix,
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
    fn parse_keeps_line_verbatim() {
        let it = parse("Firefox 火狐");
        assert_eq!(it.text, "Firefox 火狐");
        assert_eq!(it.value, "Firefox 火狐");
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