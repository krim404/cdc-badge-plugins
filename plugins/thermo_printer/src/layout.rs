//! Pure text-layout helpers: word wrapping against an arbitrary measuring
//! function (host font measurement on the badge, a character count in tests)
//! and vCard-to-print-lines extraction.

#![allow(dead_code)]

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Word-wrap `text` to lines whose measured width stays <= `max_width_px`.
///
/// `measure` returns the rendered pixel width of a candidate line. Words
/// longer than a line are hard-broken by characters. Existing newlines are
/// honoured as paragraph breaks; empty input yields no lines.
pub fn wrap_text(text: &str, max_width_px: u16, measure: &dyn Fn(&str) -> u16) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                let mut c = line.clone();
                c.push(' ');
                c.push_str(word);
                c
            };
            if measure(&candidate) <= max_width_px {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(line);
            }
            // The word alone may still be too wide: hard-break it.
            let mut piece = String::new();
            for ch in word.chars() {
                let mut c = piece.clone();
                c.push(ch);
                if measure(&c) <= max_width_px || piece.is_empty() {
                    piece = c;
                } else {
                    lines.push(piece);
                    piece = ch.to_string();
                }
            }
            line = piece;
        }
        lines.push(line);
    }
    // A single trailing empty line from empty input is noise; inner empty
    // lines (blank paragraphs) stay.
    if lines.len() == 1 && lines[0].is_empty() {
        lines.clear();
    }
    lines
}

/// One printable vCard line.
pub struct VcardLine {
    /// True for the formatted name (printed prominent).
    pub headline: bool,
    pub text: String,
}

/// Extract printable lines from vCard 4.0 text: FN as headline, then the
/// human-relevant properties in stored order. Property parameters
/// (`TEL;TYPE=cell:`) and structural lines (BEGIN/END/VERSION/N/PHOTO) are
/// dropped.
pub fn vcard_to_lines(vcard: &str) -> Vec<VcardLine> {
    let mut out = Vec::new();
    for raw in vcard.lines() {
        let line = raw.trim_end_matches('\r');
        let Some(colon) = line.find(':') else {
            continue;
        };
        let (key_full, value) = line.split_at(colon);
        let value = &value[1..];
        if value.is_empty() {
            continue;
        }
        let key = key_full
            .split(';')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        let label = match key.as_str() {
            "FN" => {
                out.push(VcardLine {
                    headline: true,
                    text: value.to_string(),
                });
                continue;
            }
            "ORG" => "Org",
            "TITLE" => "Title",
            "TEL" => "Tel",
            "EMAIL" => "Mail",
            "URL" => "Web",
            "IMPP" => "IM",
            "X-SOCIALPROFILE" => "Social",
            "NOTE" => "Note",
            _ => continue,
        };
        let mut text = String::from(label);
        text.push_str(": ");
        text.push_str(value);
        out.push(VcardLine {
            headline: false,
            text,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    // Fake measure: 6 px per character (builtin font geometry).
    fn m6(s: &str) -> u16 {
        (s.chars().count() * 6) as u16
    }

    #[test]
    fn wrap_simple_words() {
        let lines = wrap_text("aa bb cc dd", 6 * 5, &m6);
        assert_eq!(lines, vec!["aa bb", "cc dd"]);
    }

    #[test]
    fn wrap_keeps_paragraphs() {
        let lines = wrap_text("aa\n\nbb", 60, &m6);
        assert_eq!(lines, vec!["aa", "", "bb"]);
    }

    #[test]
    fn wrap_hard_breaks_long_words() {
        let lines = wrap_text("abcdefghij", 6 * 4, &m6);
        assert_eq!(lines, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn wrap_empty_input() {
        assert!(wrap_text("", 100, &m6).is_empty());
    }

    #[test]
    fn vcard_lines_extraction() {
        let v = "BEGIN:VCARD\r\nVERSION:4.0\r\nN:Doe;John;;;\r\nFN:John Doe\r\n\
                 ORG:ACME\r\nTEL;TYPE=cell:+49 123\r\nEMAIL:j@acme.org\r\nEND:VCARD\r\n";
        let lines = vcard_to_lines(v);
        assert_eq!(lines.len(), 4);
        assert!(lines[0].headline);
        assert_eq!(lines[0].text, "John Doe");
        assert_eq!(lines[1].text, "Org: ACME");
        assert_eq!(lines[2].text, "Tel: +49 123");
        assert_eq!(lines[3].text, "Mail: j@acme.org");
    }

    #[test]
    fn vcard_skips_empty_values_and_unknown_keys() {
        let v = "FN:X\nORG:\nPHOTO:data:image/jpeg;base64,xxxx\nGENDER:M\n";
        let lines = vcard_to_lines(v);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "X");
    }
}
