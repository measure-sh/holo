pub fn parse_dropbox_entries(output: &str, tag: &str, package: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let mut current_timestamp = String::new();
    let mut current_body = String::new();
    let prefix = format!(" {tag} ");

    for line in output.lines() {
        if line.len() >= 19 && line[19..].starts_with(&prefix) {
            if !current_timestamp.is_empty() && !current_body.is_empty() {
                entries.push((current_timestamp.clone(), current_body.clone()));
            }
            current_timestamp = line[..19].to_string();
            current_body.clear();
        } else if !current_timestamp.is_empty() {
            if !current_body.is_empty() {
                current_body.push('\n');
            }
            current_body.push_str(line);
        }
    }
    if !current_timestamp.is_empty() && !current_body.is_empty() {
        entries.push((current_timestamp, current_body));
    }

    if package.is_empty() {
        return entries;
    }

    entries
        .into_iter()
        .filter(|(_, body)| {
            body.lines().any(|l| {
                l.trim()
                    .strip_prefix("Process: ")
                    .is_some_and(|p| p.trim() == package)
            })
        })
        .collect()
}

pub fn extract_field(body: &str, prefix: &str) -> Option<String> {
    body.lines()
        .find_map(|l| l.trim().strip_prefix(prefix).map(|v| v.trim().to_string()))
}

pub fn extract_exception(body: &str) -> String {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.contains("Exception:") || trimmed.contains("Error:") {
            return trimmed.to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_crashes_extracts_entries() {
        let output = "\
2024-01-15 10:23:45 data_app_crash (text, 1234 bytes)
Process: com.example.app
PID: 12345
java.lang.NullPointerException: Attempt to invoke
    at com.example.app.Main.onCreate(Main.java:42)

2024-01-15 11:00:00 data_app_crash (text, 567 bytes)
Process: com.other.app
java.lang.RuntimeException: boom
    at com.other.app.Foo.bar(Foo.java:10)
";
        let entries = parse_dropbox_entries(output, "data_app_crash", "com.example.app");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "2024-01-15 10:23:45");
        assert!(entries[0].1.contains("com.example.app"));
    }

    #[test]
    fn parse_empty_package_returns_all() {
        let output = "\
2024-01-15 10:23:45 data_app_crash (text, 100 bytes)
Process: com.example.app
java.lang.NullPointerException: test

2024-01-15 11:00:00 data_app_crash (text, 100 bytes)
Process: com.other.app
java.lang.RuntimeException: test
";
        let entries = parse_dropbox_entries(output, "data_app_crash", "");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn parse_empty_output() {
        let entries = parse_dropbox_entries("", "data_app_crash", "com.example.app");
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_anrs_extracts_entries() {
        let output = "\
2024-01-15 10:23:45 data_app_anr (text, 500 bytes)
Process: com.example.app
Subject: Input dispatching timed out
PID: 12345
";
        let entries = parse_dropbox_entries(output, "data_app_anr", "com.example.app");
        assert_eq!(entries.len(), 1);
        let reason = extract_field(&entries[0].1, "Subject: ");
        assert_eq!(reason, Some("Input dispatching timed out".to_string()));
    }

    #[test]
    fn extract_exception_finds_it() {
        let body = "Process: com.example\njava.lang.NullPointerException: msg\n    at foo";
        assert!(extract_exception(body).contains("NullPointerException"));
    }
}
