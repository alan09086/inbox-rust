//! Parsing of threading-related email headers (Message-ID, In-Reply-To, References).
//!
//! All Message-IDs are trimmed, angle-bracket-stripped, and lowercased for
//! consistent lookups across the threading system.

use std::collections::HashMap;

/// Parsed threading headers from a single email.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadingHeaders {
    /// The email's own Message-ID (lowercased, bracket-stripped).
    pub message_id: Option<String>,
    /// The Message-ID this email is replying to (first ID only).
    pub in_reply_to: Option<String>,
    /// Ordered list of Message-IDs from the References header.
    /// `references[0]` is the thread root (original message).
    pub references: Vec<String>,
}

/// Extract threading headers from a header map.
///
/// Header names are matched case-insensitively per RFC 2822.
///
/// - `Message-ID`: strip angle brackets, normalize whitespace, lowercase.
/// - `In-Reply-To`: strip angle brackets, take only the first Message-ID if multiple.
/// - `References`: split on whitespace, strip angle brackets from each, preserve order.
pub fn extract_threading_headers(headers: &HashMap<String, String>) -> ThreadingHeaders {
    let mut message_id = None;
    let mut in_reply_to = None;
    let mut references = Vec::new();

    for (key, value) in headers {
        match key.to_ascii_lowercase().as_str() {
            "message-id" => {
                message_id = parse_message_id(value);
            }
            "in-reply-to" => {
                // In-Reply-To may contain multiple IDs; take only the first.
                in_reply_to = parse_references(value).into_iter().next();
            }
            "references" => {
                references = parse_references(value);
            }
            _ => {}
        }
    }

    ThreadingHeaders {
        message_id,
        in_reply_to,
        references,
    }
}

/// Construct `ThreadingHeaders` directly from pre-parsed `EmailRow` fields.
///
/// This avoids re-parsing raw headers when the individual fields are already
/// available (e.g., from the database).
pub fn threading_headers_from_fields(
    message_id_header: Option<&str>,
    in_reply_to: Option<&str>,
    references_json: Option<&str>,
) -> ThreadingHeaders {
    let message_id = message_id_header.and_then(parse_message_id);
    let irt = in_reply_to.and_then(|v| {
        // The in_reply_to field may already be bracket-stripped in the DB,
        // but we normalize just in case.
        parse_message_id(v)
    });

    let refs = references_json
        .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
        .unwrap_or_default()
        .iter()
        .filter_map(|r| parse_message_id(r))
        .collect();

    ThreadingHeaders {
        message_id,
        in_reply_to: irt,
        references: refs,
    }
}

/// Parse a single Message-ID value: strip angle brackets, trim, lowercase.
///
/// Returns `None` if the input is empty or only whitespace after stripping.
fn parse_message_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Extract content between angle brackets if present.
    // Handle cases like "<abc@example.com>" or bare "abc@example.com".
    let extracted = if let Some(start) = trimmed.find('<') {
        if let Some(end) = trimmed[start..].find('>') {
            &trimmed[start + 1..start + end]
        } else {
            // Opening bracket but no closing — take everything after '<'
            &trimmed[start + 1..]
        }
    } else {
        trimmed
    };

    let result = extracted.trim().to_ascii_lowercase();
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Parse a References header value into an ordered list of Message-IDs.
///
/// References is a space-separated list of angle-bracket-delimited Message-IDs.
/// Example: `"<a@example.com> <b@example.com> <c@example.com>"`
///
/// Also handles bare IDs without brackets: `"a@example.com b@example.com"`.
/// Handles header folding (tabs, newlines treated as whitespace).
fn parse_references(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // Check if the value contains angle brackets.
    if trimmed.contains('<') {
        // Extract all <...> tokens.
        let mut results = Vec::new();
        let mut rest = trimmed;
        while let Some(start) = rest.find('<') {
            if let Some(end) = rest[start..].find('>') {
                let content = rest[start + 1..start + end].trim().to_ascii_lowercase();
                if !content.is_empty() {
                    results.push(content);
                }
                rest = &rest[start + end + 1..];
            } else {
                // Unclosed bracket — take everything after '<'
                let content = rest[start + 1..].trim().to_ascii_lowercase();
                if !content.is_empty() {
                    results.push(content);
                }
                break;
            }
        }
        results
    } else {
        // No angle brackets — split on whitespace, treat each token as a bare ID.
        trimmed
            .split_whitespace()
            .filter_map(|token| {
                let id = token.trim().to_ascii_lowercase();
                if id.is_empty() { None } else { Some(id) }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn standard_headers() {
        let h = headers(&[
            ("Message-ID", "<abc@example.com>"),
            ("In-Reply-To", "<parent@example.com>"),
            ("References", "<root@ex.com> <mid@ex.com>"),
        ]);
        let th = extract_threading_headers(&h);
        assert_eq!(th.message_id, Some("abc@example.com".into()));
        assert_eq!(th.in_reply_to, Some("parent@example.com".into()));
        assert_eq!(th.references, vec!["root@ex.com", "mid@ex.com"]);
    }

    #[test]
    fn missing_all_headers() {
        let h: HashMap<String, String> = HashMap::new();
        let th = extract_threading_headers(&h);
        assert_eq!(th.message_id, None);
        assert_eq!(th.in_reply_to, None);
        assert!(th.references.is_empty());
    }

    #[test]
    fn only_in_reply_to() {
        let h = headers(&[("In-Reply-To", "<only@ex.com>")]);
        let th = extract_threading_headers(&h);
        assert_eq!(th.message_id, None);
        assert_eq!(th.in_reply_to, Some("only@ex.com".into()));
        assert!(th.references.is_empty());
    }

    #[test]
    fn references_with_inconsistent_whitespace() {
        let h = headers(&[("References", "<a@ex.com>\t\n  <b@ex.com>   <c@ex.com>")]);
        let th = extract_threading_headers(&h);
        assert_eq!(th.references, vec!["a@ex.com", "b@ex.com", "c@ex.com"]);
    }

    #[test]
    fn bare_ids_without_angle_brackets() {
        let h = headers(&[("References", "a@example.com b@example.com")]);
        let th = extract_threading_headers(&h);
        assert_eq!(
            th.references,
            vec!["a@example.com", "b@example.com"]
        );
    }

    #[test]
    fn multiple_in_reply_to_takes_first() {
        let h = headers(&[("In-Reply-To", "<first@ex.com> <second@ex.com>")]);
        let th = extract_threading_headers(&h);
        assert_eq!(th.in_reply_to, Some("first@ex.com".into()));
    }

    #[test]
    fn case_insensitive_header_lookup() {
        let h = headers(&[("message-id", "<lower@ex.com>")]);
        let th = extract_threading_headers(&h);
        assert_eq!(th.message_id, Some("lower@ex.com".into()));

        let h2 = headers(&[("MESSAGE-ID", "<UPPER@EX.COM>")]);
        let th2 = extract_threading_headers(&h2);
        assert_eq!(th2.message_id, Some("upper@ex.com".into()));
    }

    #[test]
    fn duplicate_message_ids_in_references_preserved() {
        let h = headers(&[("References", "<a@ex.com> <b@ex.com> <a@ex.com>")]);
        let th = extract_threading_headers(&h);
        assert_eq!(th.references, vec!["a@ex.com", "b@ex.com", "a@ex.com"]);
    }

    #[test]
    fn empty_message_id_returns_none() {
        assert_eq!(parse_message_id(""), None);
        assert_eq!(parse_message_id("   "), None);
        assert_eq!(parse_message_id("<>"), None);
    }

    #[test]
    fn empty_references_returns_empty() {
        assert!(parse_references("").is_empty());
        assert!(parse_references("   \t  \n  ").is_empty());
    }

    #[test]
    fn missing_closing_bracket() {
        let h = headers(&[("References", "<abc@example.com")]);
        let th = extract_threading_headers(&h);
        assert_eq!(th.references, vec!["abc@example.com"]);
    }

    #[test]
    fn threading_headers_from_fields_basic() {
        let th = threading_headers_from_fields(
            Some("<ABC@Example.com>"),
            Some("<parent@ex.com>"),
            Some(r#"["<root@ex.com>", "<mid@ex.com>"]"#),
        );
        assert_eq!(th.message_id, Some("abc@example.com".into()));
        assert_eq!(th.in_reply_to, Some("parent@ex.com".into()));
        assert_eq!(th.references, vec!["root@ex.com", "mid@ex.com"]);
    }

    #[test]
    fn threading_headers_from_fields_all_none() {
        let th = threading_headers_from_fields(None, None, None);
        assert_eq!(th.message_id, None);
        assert_eq!(th.in_reply_to, None);
        assert!(th.references.is_empty());
    }

    #[test]
    fn threading_headers_from_fields_invalid_json() {
        let th = threading_headers_from_fields(None, None, Some("not json"));
        assert!(th.references.is_empty());
    }
}
