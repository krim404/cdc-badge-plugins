//! Tiny HTTP/1.0 helpers: request-line parsing and content-type mapping.
//! Pure, dependency-free, unit-tested on the host.

#![allow(dead_code)]

use alloc::string::String;

/// Outcome of parsing an HTTP request line.
pub enum Request {
    /// A well-formed GET with its decoded path.
    Get(String),
    /// Well-formed line, but a method this server does not implement (405).
    OtherMethod,
    /// Not parseable as an HTTP request line (400).
    Bad,
}

/// Parse an HTTP request's first line.
///
/// `GET /foo/bar.html?x=1 HTTP/1.1` becomes `Request::Get("/foo/bar.html")`.
/// The query string is dropped and `%NN` escapes are decoded; a decode to
/// invalid UTF-8 counts as `Bad`.
pub fn parse_request(request: &str) -> Request {
    let Some(line) = request.split(['\r', '\n']).next() else {
        return Request::Bad;
    };
    let mut parts = line.split(' ');
    let Some(method) = parts.next() else {
        return Request::Bad;
    };
    let Some(target) = parts.next() else {
        return Request::Bad;
    };
    if !method.eq_ignore_ascii_case("GET") {
        // Anything else that still looks like "<METHOD> <target> ..." is a
        // valid request for something this server does not do: 405, not 404.
        let known = method.chars().all(|c| c.is_ascii_alphabetic()) && !method.is_empty();
        return if known {
            Request::OtherMethod
        } else {
            Request::Bad
        };
    }
    let path = target.split('?').next().unwrap_or(target);
    if path.is_empty() || !path.starts_with('/') {
        return Request::Bad;
    }
    match percent_decode(path) {
        Some(p) => Request::Get(p),
        None => Request::Bad,
    }
}

/// Map the local file path ("/" → index) to a served file name relative to the
/// plugin's vFAT folder. Rejects path traversal (`..`). Returns `None` when the
/// path is unsafe.
pub fn resolve_file(path: &str) -> Option<String> {
    let trimmed = path.trim_start_matches('/');
    let name = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };
    if name.split('/').any(|seg| seg == ".." || seg == ".") {
        return None;
    }
    Some(String::from(name))
}

/// Content-Type for a file name by extension; defaults to octet-stream.
pub fn content_type(name: &str) -> &'static str {
    let lower_ext = name.rsplit('.').next().unwrap_or("");
    match lower_ext.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html",
        "txt" | "md" => "text/plain",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// Decode `%NN` escapes over bytes, then re-validate as UTF-8: decoding into
/// chars directly would read the bytes as Latin-1 and mangle multi-byte
/// sequences (`%C3%A4` must become "ä", not "Ã¤").
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = alloc::vec::Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn get_path(req: &str) -> Option<String> {
        match parse_request(req) {
            Request::Get(p) => Some(p),
            _ => None,
        }
    }

    #[test]
    fn get_path_basic() {
        assert_eq!(get_path("GET / HTTP/1.1\r\n"), Some("/".to_string()));
        assert_eq!(
            get_path("GET /a.html HTTP/1.0\r\n"),
            Some("/a.html".to_string())
        );
    }

    #[test]
    fn get_path_strips_query() {
        assert_eq!(
            get_path("GET /x.png?v=2 HTTP/1.1"),
            Some("/x.png".to_string())
        );
    }

    #[test]
    fn get_path_percent_decode() {
        assert_eq!(
            get_path("GET /a%20b.txt HTTP/1.1"),
            Some("/a b.txt".to_string())
        );
        // Multi-byte UTF-8 escapes must decode over bytes, not per-char.
        assert_eq!(
            get_path("GET /%C3%A4.txt HTTP/1.1"),
            Some("/ä.txt".to_string())
        );
    }

    #[test]
    fn non_get_is_method_not_allowed() {
        assert!(matches!(
            parse_request("POST / HTTP/1.1"),
            Request::OtherMethod
        ));
        assert!(matches!(
            parse_request("HEAD / HTTP/1.1"),
            Request::OtherMethod
        ));
        assert!(matches!(parse_request("garbage"), Request::Bad));
        assert!(matches!(parse_request(""), Request::Bad));
    }

    #[test]
    fn resolve_index_and_traversal() {
        assert_eq!(resolve_file("/"), Some("index.html".to_string()));
        assert_eq!(resolve_file("/foo.html"), Some("foo.html".to_string()));
        assert_eq!(resolve_file("/../secret"), None);
        assert_eq!(resolve_file("/a/../b"), None);
    }

    #[test]
    fn content_types() {
        assert_eq!(content_type("index.html"), "text/html");
        assert_eq!(content_type("style.CSS"), "text/css");
        assert_eq!(content_type("pic.JPG"), "image/jpeg");
        assert_eq!(content_type("data.bin"), "application/octet-stream");
        assert_eq!(content_type("noext"), "application/octet-stream");
    }
}
