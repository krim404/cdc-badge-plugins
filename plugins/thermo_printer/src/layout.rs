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

/// One laid-out page line: headline flag and text.
pub type PageLine = (bool, String);

/// Lay out one page from `lines`: each source line is wrapped with
/// `wrap(headline, text)` and advances `y` by `line_h(headline)` per wrapped
/// row, until `max_rows` is reached. Consumed source lines are drained. Blank
/// source lines occupy one row and are consumed even at the page bottom. A
/// source line that does not fit on an otherwise empty page is split: the rows
/// that fit are emitted and the remainder stays queued as the new first line,
/// so every call makes progress. Returns the page rows and the final `y`.
pub fn paginate(
    lines: &mut Vec<VcardLine>,
    y0: i32,
    max_rows: i32,
    line_h: &dyn Fn(bool) -> i32,
    wrap: &dyn Fn(bool, &str) -> Vec<String>,
) -> (Vec<PageLine>, i32) {
    let mut page: Vec<PageLine> = Vec::new();
    let mut y = y0;
    let mut consumed = 0usize;
    while let Some(line) = lines.get(consumed) {
        let headline = line.headline;
        let h = line_h(headline).max(1);
        let wrapped = wrap(headline, &line.text);
        if wrapped.is_empty() {
            page.push((headline, String::new()));
            y += h;
            consumed += 1;
            continue;
        }
        let fit = ((max_rows - y) / h).max(0) as usize;
        if wrapped.len() <= fit {
            for w in wrapped {
                page.push((headline, w));
                y += h;
            }
            consumed += 1;
            continue;
        }
        if consumed == 0 {
            let take = fit.max(1);
            for w in wrapped.iter().take(take) {
                page.push((headline, w.clone()));
                y += h;
            }
            // The leftover rows already fit a line each; queue them as-is so
            // hard-broken words are not re-joined with spaces.
            let rest = wrapped[take..].iter().map(|w| VcardLine {
                headline,
                text: w.clone(),
            });
            lines.splice(0..1, rest);
        }
        break;
    }
    lines.drain(..consumed);
    (page, y)
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

    fn wrap6(_headline: bool, text: &str) -> Vec<String> {
        wrap_text(text, 6 * 10, &m6)
    }

    fn src(texts: &[&str]) -> Vec<VcardLine> {
        texts
            .iter()
            .map(|t| VcardLine {
                headline: false,
                text: t.to_string(),
            })
            .collect()
    }

    #[test]
    fn paginate_fills_page_and_keeps_rest() {
        let mut lines = src(&["aa bb", "cc", "dd ee", "ff"]);
        let (page, y) = paginate(&mut lines, 0, 20, &|_| 10, &wrap6);
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].1, "aa bb");
        assert_eq!(page[1].1, "cc");
        assert_eq!(y, 20);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "dd ee");
    }

    #[test]
    fn paginate_splits_oversized_first_line() {
        // 24 words wrap to 8 rows of 10 px; a 40 px page takes 4 rows per call.
        let long = (0..12).map(|_| "ab cd").collect::<Vec<_>>().join(" ");
        let mut lines = src(&[&long, "tail"]);
        let mut pages = 0;
        let mut rows = 0;
        while !lines.is_empty() {
            let (page, _) = paginate(&mut lines, 0, 40, &|_| 10, &wrap6);
            assert!(!page.is_empty(), "every page must make progress");
            rows += page.len();
            pages += 1;
            assert!(pages <= 10, "pagination must terminate");
        }
        assert_eq!(pages, 3);
        assert_eq!(rows, 9);
    }

    #[test]
    fn paginate_split_keeps_hard_broken_words_intact() {
        let mut lines = src(&["abcdefghijklmnopqrstuvwxyz"]);
        let (page, _) = paginate(&mut lines, 0, 10, &|_| 10, &wrap6);
        assert_eq!(page[0].1, "abcdefghij");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "klmnopqrst");
        assert_eq!(lines[1].text, "uvwxyz");
    }

    #[test]
    fn paginate_blank_line_consumed_at_bottom() {
        let mut lines = src(&["aa", "", "bb"]);
        // The blank line is consumed on the full page; "bb" waits.
        let (page, _) = paginate(&mut lines, 0, 10, &|_| 10, &wrap6);
        assert_eq!(page.len(), 2);
        assert_eq!(page[1].1, "");
        assert_eq!(lines.len(), 1);
        let (page, _) = paginate(&mut lines, 0, 10, &|_| 10, &wrap6);
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].1, "bb");
        assert!(lines.is_empty());
    }

    #[test]
    fn paginate_empty_input() {
        let mut lines: Vec<VcardLine> = Vec::new();
        let (page, y) = paginate(&mut lines, 4, 100, &|_| 10, &wrap6);
        assert!(page.is_empty());
        assert_eq!(y, 4);
    }
}
