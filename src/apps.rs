pub fn fuzzy_matches(name: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let name_lower = name.to_ascii_lowercase();
    let query_lower = query.to_ascii_lowercase();
    let mut chars = query_lower.chars();
    let mut current = chars.next();
    for c in name_lower.chars() {
        if current == Some(c) {
            current = chars.next();
        }
    }
    current.is_none()
}

pub fn filtered_packages<'a>(packages: &'a [String], query: &str) -> Vec<&'a str> {
    packages
        .iter()
        .filter(|p| fuzzy_matches(p, query))
        .map(|s| s.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches_empty_query() {
        assert!(fuzzy_matches("com.example.app", ""));
    }

    #[test]
    fn fuzzy_matches_exact() {
        assert!(fuzzy_matches("com.example.app", "com.example.app"));
    }

    #[test]
    fn fuzzy_matches_subsequence() {
        assert!(fuzzy_matches("com.example.app", "cexa"));
    }

    #[test]
    fn fuzzy_matches_case_insensitive() {
        assert!(fuzzy_matches("com.Example.App", "exa"));
        assert!(fuzzy_matches("com.example.app", "EXA"));
    }

    #[test]
    fn fuzzy_no_match() {
        assert!(!fuzzy_matches("com.example.app", "xyz"));
    }

    #[test]
    fn fuzzy_query_longer_than_name() {
        assert!(!fuzzy_matches("ab", "abc"));
    }

    #[test]
    fn filtered_packages_returns_matches() {
        let pkgs = vec![
            "com.spotify.music".to_string(),
            "com.whatsapp".to_string(),
            "com.android.chrome".to_string(),
        ];
        let result = filtered_packages(&pkgs, "spot");
        assert_eq!(result, vec!["com.spotify.music"]);
    }

    #[test]
    fn filtered_packages_empty_query_returns_all() {
        let pkgs = vec!["a".to_string(), "b".to_string()];
        assert_eq!(filtered_packages(&pkgs, "").len(), 2);
    }
}
