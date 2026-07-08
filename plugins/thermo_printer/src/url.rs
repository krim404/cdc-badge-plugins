//! Conservative URL normalisation for the free-text QR flow.
//!
//! `http://` / `https://` inputs stay unchanged; a bare `host.tld[/path]`
//! gets an `https://` prefix; everything else is treated as plain text and
//! never modified.

#![allow(dead_code)]

use alloc::format;
use alloc::string::String;

/// Result of normalising user input for QR encoding.
#[derive(Debug, PartialEq, Eq)]
pub enum Normalized {
    /// The input is a URL; encode this exact string.
    Url(String),
    /// Plain text; encode verbatim.
    Text(String),
}

/// Normalise T9 input: URLs pass through / get an `https://` prefix, plain
/// text stays untouched.
pub fn normalize(input: &str) -> Normalized {
    let trimmed = input.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Normalized::Url(String::from(trimmed));
    }
    if looks_like_domain(trimmed) {
        return Normalized::Url(format!("https://{trimmed}"));
    }
    Normalized::Text(String::from(input))
}

/// Conservative `host.tld[/path]` heuristic: at least one dot in the host,
/// only hostname characters before the first slash, a plausible TLD, and no
/// whitespace anywhere.
fn looks_like_domain(s: &str) -> bool {
    if s.is_empty() || s.chars().any(char::is_whitespace) {
        return false;
    }
    let host = s.split('/').next().unwrap_or("");
    if !host.contains('.') || host.starts_with('.') || host.ends_with('.') {
        return false;
    }
    let mut labels = host.split('.');
    let valid_label = |l: &str| {
        !l.is_empty()
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    if !labels.all(valid_label) {
        return false;
    }
    // The TLD must be alphabetic and at least 2 chars ("example.1" is text).
    let tld = host.rsplit('.').next().unwrap_or("");
    tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Normalized {
        Normalized::Url(String::from(s))
    }
    fn text(s: &str) -> Normalized {
        Normalized::Text(String::from(s))
    }

    #[test]
    fn explicit_schemes_unchanged() {
        assert_eq!(normalize("http://example.org"), url("http://example.org"));
        assert_eq!(
            normalize("https://example.org/path?q=1"),
            url("https://example.org/path?q=1")
        );
    }

    #[test]
    fn bare_domains_get_https() {
        assert_eq!(normalize("example.org"), url("https://example.org"));
        assert_eq!(
            normalize("example.org/path"),
            url("https://example.org/path")
        );
        assert_eq!(
            normalize("sub.example.co/x/y"),
            url("https://sub.example.co/x/y")
        );
        assert_eq!(normalize("  example.org  "), url("https://example.org"));
    }

    #[test]
    fn plain_text_untouched() {
        assert_eq!(normalize("hello world"), text("hello world"));
        assert_eq!(normalize("no domain here"), text("no domain here"));
        assert_eq!(normalize("1.5"), text("1.5"));
        assert_eq!(normalize("version 2.0 rocks"), text("version 2.0 rocks"));
        assert_eq!(normalize(".org"), text(".org"));
        assert_eq!(normalize("org."), text("org."));
        assert_eq!(normalize("-bad.org"), text("-bad.org"));
        assert_eq!(normalize(""), text(""));
    }

    #[test]
    fn whitespace_inside_is_text() {
        assert_eq!(normalize("exa mple.org"), text("exa mple.org"));
    }
}
